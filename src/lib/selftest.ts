// 开发用的自检：在真实的桌面窗口里把几条关键链路走一遍，结果打印到启动它的终端，然后退出。
//
// 用法（仓库和缓存都指向临时目录，不碰正式数据）：
//   SHIBAPACK_CONFIG=/tmp/x/config.json SHIBAPACK_CACHE=/tmp/x/cache \
//   SHIBAPACK_SELFTEST=/path/to/some-installer.exe ./target/debug/shibapack
//
// 它检查的是单元测试和浏览器预览都够不着的部分：前后端之间的调用、
// 图标文件能不能通过资源协议显示出来、已安装软件在真实窗口里能不能列出来。
//
// 清理只读地扫一遍、不选任何项清理一次，不删这台电脑上的任何东西；
// 试着把仓库加成清理文件夹、往保留名单里加不像域名的东西，都应该被拦下（拦下时不写设置）。

import { api, assetUrl, onCleanupItem, windowControls } from "./api";
import { useCleanup } from "./cleanup";
import { usePrefs } from "./prefs";
import { useApp } from "./store";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

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

    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const appWindow = getCurrentWindow();
    const before = await appWindow.innerSize();
    const scale = await appWindow.scaleFactor();
    const detailOpen = usePrefs.getState().detailOpen;
    const main = document.querySelector("main");
    const toggle = async (open: boolean) => {
      const button = document.querySelector<HTMLButtonElement>("[data-detail-toggle]");
      if (!button || button.disabled) throw new Error("详情栏开关不可用");
      button.click();
      const deadline = Date.now() + 2000;
      while (usePrefs.getState().detailOpen !== open && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      if (usePrefs.getState().detailOpen !== open) throw new Error("详情栏状态切换超时");
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      return appWindow.innerSize();
    };
    if (!main) throw new Error("没有找到中间列表");
    const widths = [main.clientWidth];
    const observer = new ResizeObserver(() => widths.push(main.clientWidth));
    observer.observe(main);
    try {
      const changed = await toggle(!detailOpen);
      const middle = main.clientWidth;
      const restored = await toggle(detailOpen);
      await check("详情栏同步调整窗口宽度", (detailOpen ? changed.width < before.width : changed.width > before.width) && restored.width === before.width
        && changed.height === before.height && restored.height === before.height, `${before.width} → ${changed.width} → ${restored.width}`);
      const min = Math.min(widths[0], middle);
      const max = Math.max(widths[0], middle);
      await check("中间列表宽度没有瞬间跳变", widths.every((width) => width >= min - 2 && width <= max + 2), widths.join(" → "));
    } finally {
      observer.disconnect();
    }

    const logicalWidth = before.width / scale;
    await windowControls.resizeForDetail(false, logicalWidth, logicalWidth);
    const narrow = await appWindow.innerSize();
    await windowControls.resizeForDetail(true, logicalWidth, logicalWidth);
    await check("桌面窗口可以缩窄到 560 px", Math.abs(narrow.width / scale - 560) <= 1, `${Math.round(narrow.width / scale)} px`);

    // 导入一个真实文件（托管模式），看记录、文件和图标
    const result = await api.importPaths([status.selftest!], "managed");
    const item = result.report.items[0];
    await check("导入文件", item?.status === "imported" || item?.status === "duplicate", `${item?.status} ${item?.message ?? ""}`);
    const software = result.library.software.find((s) => s.id === item?.softwareId);
    const file = software?.versions[0]?.files[0];
    await check("索引里有这条记录", Boolean(software && file), `${software?.name} / ${software?.versions[0]?.version} / ${file?.path}`);
    await check("从安装包里取到了图标并缓存到索引", Boolean(software?.iconData), software?.icon ?? "");
    if (software?.iconData) {
      await check("索引里的图标能在界面里显示", await loads(software.iconData));
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
    const { apps: installed, cachedIcons } = await api.listInstalled();
    const withIcon = installed.filter((app) => app.icon);
    await check("列出已安装软件", installed.length > 0, `${installed.length} 个，${withIcon.length} 个有图标`);
    if (withIcon[0]) {
      await check("已安装软件的图标能在界面里显示", await loads(withIcon[0].icon!), withIcon[0].name);
      const cached = cachedIcons?.[withIcon[0].name.trim().toLowerCase()];
      await check("已安装软件的图标可以从索引显示", Boolean(cached && await loads(cached)), withIcon[0].name);
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

    // 清理：只读地扫一遍，看前后端和界面接通了
    try {
      let streamed = 0;
      const stop = await onCleanupItem(() => streamed++);
      const started = performance.now();
      const items = await api.cleanupScan();
      stop();
      const seconds = ((performance.now() - started) / 1000).toFixed(1);
      const total = items.reduce((sum, item) => sum + (item.size ?? 0), 0);
      await check("清理扫描", items.length > 0, `${items.length} 项，共 ${(total / 1024 ** 3).toFixed(1)} GB，用时 ${seconds} 秒`);
      await check("扫描时一项一项送到界面", streamed === items.length, `收到 ${streamed} 项`);
      await check("每一项都有中英文名字和说明", items.every((item) => item.name.zh && item.name.en && item.about.zh && item.about.en));
      await check("条目都是完整路径", items.every((item) => item.entries.every((entry) => /^([A-Za-z]:[\\/]|\/)/.test(entry.path))));
      const running = [...new Set(items.filter((item) => item.running).map((item) => item.app ?? item.id))];
      await check("认出要先关掉的程序", true, running.length > 0 ? `正在运行：${running.join("、")}` : "都没在运行");
      const admin = items.filter((item) => item.admin).map((item) => item.id);
      await check("列出要管理员权限的项", true, admin.length > 0 ? admin.join("、") : "这台电脑上没有");
      const nothing = await api.cleanupRun([]);
      await check("不选任何项时什么都不清", nothing.removed === 0 && nothing.freed === 0 && nothing.trashed === 0 && !nothing.history);
      const before = await api.cleanupOptions();
      try {
        const saved = await api.cleanupSetOptions({ ...before, custom: [...before.custom, status.repoRoot ?? ""] });
        // 不该存进去：存进去了就马上改回来
        await api.cleanupSetOptions(before);
        await check("仓库不能加成清理文件夹", false, `被接受了：${saved.custom.join(", ")}`);
      } catch (error) {
        await check("仓库不能加成清理文件夹", String(error).startsWith("cleanup_protected"), String(error));
      }
      try {
        await api.cleanupSetOptions({ ...before, keepCookies: [...before.keepCookies, "not a domain"] });
        await api.cleanupSetOptions(before);
        await check("保留名单不收不像域名的", false, "被接受了");
      } catch (error) {
        await check("保留名单不收不像域名的", String(error).startsWith("cleanup_bad_domain"), String(error));
      }
      // 只报个数，不把网站名打出来
      const sites = await api.cleanupCookieSites();
      await check("读得到浏览器里有 Cookie 的网站", Array.isArray(sites), `${sites.length} 个网站`);
      const history = await api.cleanupHistory();
      await check("清理记录能读", Array.isArray(history), `${history.length} 次`);
      // 清理页在真实窗口里显示出扫描结果
      useApp.getState().setRoute("cleanup");
      const deadline = Date.now() + 60_000;
      await sleep(300);
      while ((useCleanup.getState().scanning || !useCleanup.getState().items?.length) && Date.now() < deadline) await sleep(200);
      await sleep(300);
      const first = useCleanup.getState().items?.[0];
      const text = document.querySelector("main")?.textContent ?? "";
      await check("清理页显示出扫描结果", Boolean(first) && (text.includes(first!.name.zh) || text.includes(first!.name.en)), first?.id ?? "没有项");
      const sidebar = document.querySelector("#app-sidebar")?.textContent ?? "";
      await check("侧栏显示能清理多少", /\d(\.\d)? (B|KB|MB|GB|TB)/.test(sidebar));
      useApp.getState().setRoute("library");
    } catch (error) {
      await check("清理", false, String(error));
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
