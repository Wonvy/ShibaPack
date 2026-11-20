// 开发用的自检：在真实的桌面窗口里把几条关键链路走一遍，结果打印到启动它的终端，然后退出。
//
// 用法（仓库和缓存都指向临时目录，不碰正式数据）：
//   SHIBAPACK_CONFIG=/tmp/x/config.json SHIBAPACK_CACHE=/tmp/x/cache \
//   SHIBAPACK_SELFTEST=/path/to/some-installer.exe ./target/debug/shibapack
//
// 它检查的是单元测试和浏览器预览都够不着的部分：前后端之间的调用、
// 图标文件能不能通过资源协议显示出来、已安装软件在真实窗口里能不能列出来。

import { api, assetUrl } from "./api";
import { useApp } from "./store";

function loads(path: string): Promise<boolean> {
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve(image.naturalWidth > 0);
    image.onerror = () => resolve(false);
    image.src = assetUrl(path);
    setTimeout(() => resolve(false), 5000);
  });
}

export async function runSelftest() {
  const report = (line: string) => api.selftestReport(line);
  let failures = 0;
  const check = async (name: string, ok: boolean, detail = "") => {
    if (!ok) failures++;
    await report(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  };

  try {
    const status = await api.getStatus();
    await check("仓库已打开", status.configured, status.repoRoot ?? "");

    // 导入一个真实文件（托管模式），看记录、文件和图标
    const result = await api.importPaths([status.selftest!], "managed");
    const item = result.report.items[0];
    await check("导入文件", item?.status === "imported" || item?.status === "duplicate", `${item?.status} ${item?.message ?? ""}`);
    const software = result.library.software.find((s) => s.id === item?.softwareId);
    const file = software?.versions[0]?.files[0];
    await check("索引里有这条记录", Boolean(software && file), `${software?.name} / ${software?.versions[0]?.version} / ${file?.path}`);
    await check("从安装包里取到了图标", Boolean(software?.icon), software?.icon ?? "");
    if (software?.icon) {
      await check("仓库里的图标能在界面里显示", await loads(`${status.repoRoot}/${software.icon}`));
    }

    // 修改再读回，确认写入生效
    if (software) {
      const edited = await api.updateSoftware(software.id, {
        name: software.name,
        publisher: "Selftest Publisher",
        homepage: "",
        note: "",
        categoryId: null,
        starred: true,
        defaultVersionId: null,
      });
      const reread = (await api.getLibrary()).software.find((s) => s.id === software.id);
      await check("修改后能读回", edited.software.length > 0 && reread?.publisher === "Selftest Publisher" && reread.starred);
    }

    const stats = await api.getStats();
    await check("统计", stats.files >= 1, `${stats.software} 个软件，${stats.files} 个文件`);
    const backups = await api.createBackup();
    await check("立即备份", backups.length >= 1, `${backups.length} 份`);
    const verify = await api.verifyData();
    await check("校验文件", verify.problems.length === 0, `检查 ${verify.checked} 个，问题 ${verify.problems.length} 个`);

    // 已安装软件
    const installed = await api.listInstalled();
    const withIcon = installed.filter((app) => app.icon);
    await check("列出已安装软件", installed.length > 0, `${installed.length} 个，${withIcon.length} 个有图标`);
    if (withIcon[0]) {
      await check("已安装软件的图标能在界面里显示", await loads(withIcon[0].icon!), withIcon[0].name);
    }
    const links = await api.matchInstalled();
    await check("对应关系可以取到", typeof links === "object", `${Object.keys(links).length} 个软件已安装`);

    // 启动项与服务：只列，不改（自检不碰这台电脑的设置）。自检时不列 macOS 的登录项，免得弹出授权提示
    try {
      const startup = await api.listStartup();
      const services = startup.items.filter((item) => item.kind === "service");
      const thirdParty = startup.items.filter((item) => !item.system);
      await check(
        "列出启动项和服务",
        startup.items.length > 0 && startup.notes.length === 0,
        `${startup.items.length} 项（服务 ${services.length} 个，第三方 ${thirdParty.length} 个）${startup.notes.join(" ")}`,
      );
      const running = startup.items.filter((item) => item.running);
      await check("能看出哪些在运行", running.length > 0, `${running.length} 个正在运行`);
      const iconItem = startup.items.find((item) => item.icon);
      if (iconItem) await check("启动项的图标能在界面里显示", await loads(iconItem.icon!), `${iconItem.name} → ${iconItem.app}`);
      // 启用、停用、定位都按 id 找，再列一遍 id 要对得上
      const first = thirdParty[0] ?? startup.items[0];
      if (first) {
        const again = await api.listStartup();
        await check("再列一遍 id 不变", again.items.some((item) => item.id === first.id), first.id);
      }
    } catch (error) {
      await check("列出启动项和服务", false, String(error));
    }

    // 整理根目录：启动前在仓库根目录放了一个散落的安装包
    const tidy = await api.tidyRoot();
    await check("整理根目录", tidy.report.failed === 0, `归位 ${tidy.report.imported} 个，重复 ${tidy.report.duplicates} 个`);

    // 在线源（需要联网）：查询、关联、检查更新
    try {
      const release = await api.sourceLookup("homebrew", "obsidian");
      await check("在线源查询", release.assets.length > 0, `Homebrew obsidian → ${release.version}，${release.assets.length} 个安装包`);
      const hits = await api.sourceSearch("obsidian");
      await check("在线源搜索", hits.some((hit) => hit.reference === "obsidian"), `${hits.length} 条结果`);
      const added = await api.subscribe(null, "Obsidian", "homebrew", "obsidian");
      const subscribed = added.library.software.find((s) => s.id === added.softwareId);
      await check("从在线源添加软件", subscribed?.subscription?.latest?.version === release.version, subscribed?.name ?? "");
      const checked = await api.checkUpdates();
      await check("检查更新", checked.checked >= 1 && !checked.rateLimited, `查到 ${checked.checked} 个，失败 ${checked.failed} 个`);
    } catch (error) {
      await check("在线源", false, String(error));
    }

    // 按名字取简介（已安装软件的详情栏也用它），需要时顺带翻译成界面语言
    try {
      const library = await api.fetchSummary("Git", true);
      const summary = library.summaries["Git"];
      const shown = summary?.translations?.[library.settings.language] ?? summary?.text ?? "";
      await check("按名字取简介", shown.length > 0, `${summary?.source ?? ""} · ${shown.slice(0, 40)}`);
    } catch (error) {
      await check("按名字取简介", false, String(error));
    }

    // 智能分类：先出建议，再按建议分类
    try {
      const all = (await api.getLibrary()).software;
      const suggestions = await api.suggestCategories(all.map((s) => s.id));
      const usable = suggestions.filter((s) => s.categoryId || s.newCategory);
      const assignments = usable.map((s) =>
        s.categoryId ? { softwareId: s.softwareId, categoryId: s.categoryId } : { softwareId: s.softwareId, newCategory: s.newCategory! },
      );
      const after = await api.applyClassification(assignments);
      const placed = usable.filter((s) => after.software.find((x) => x.id === s.softwareId)?.categoryId).length;
      await check("智能分类", usable.length > 0 && placed === usable.length, `建议 ${usable.length} 个，已归类 ${placed} 个，分类 ${after.categories.map((c) => c.name).join("、")}`);
    } catch (error) {
      await check("智能分类", false, String(error));
    }

    // 语言包：导入自检文件旁边的 lang.json
    const packFile = status.selftest!.replace(/[^/\\]+$/, "lang.json");
    try {
      const packs = await api.importLangPack(packFile);
      useApp.getState().useLangPacks(packs);
      await check("导入语言包", packs.some((pack) => pack._locale === "ja-JP"), packs.map((pack) => pack._locale).join(", "));
    } catch (error) {
      await check("导入语言包", false, String(error));
    }

    // 应用自己的在线更新：能查就行。还没发布过版本时更新地址不存在，这属于正常；
    // 只有权限没给或配置写错（查都不让查）才算失败
    try {
      const { check: checkUpdate } = await import("@tauri-apps/plugin-updater");
      const found = await checkUpdate();
      await check("在线更新可以检查", true, found ? `有新版本 ${found.version}` : "已是最新");
    } catch (error) {
      const text = String(error);
      await check("在线更新可以检查", !/not allowed|permission|pubkey|capabilit/i.test(text), text);
    }

    // 界面状态和数据是否一致
    const state = useApp.getState();
    await check("界面已加载索引", state.library !== null);
  } catch (error) {
    failures++;
    await report(`FAIL 自检中断 — ${String(error)}`);
  }
  await api.selftestReport(failures === 0 ? "全部通过" : `${failures} 项失败`, true);
}
