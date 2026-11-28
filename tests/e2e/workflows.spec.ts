import { expect, test, type Page } from "@playwright/test";

async function chooseImport(page: Page) {
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await page.getByRole("menuitem", { name: "导入文件…" }).click();
  return page.getByRole("dialog");
}

test("停止正在导入的批次后，可以重新导入", async ({ page }) => {
  await page.goto("/?empty#/library");
  let dialog = await chooseImport(page);
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await page.getByRole("dialog", { name: "正在导入" }).getByRole("button", { name: "停止", exact: true }).click();
  await expect(dialog.getByText("导入已停止，已完成的项目会保留")).toBeVisible();
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(page.locator("[data-software-id]")).toHaveCount(0);

  dialog = await chooseImport(page);
  await dialog.getByRole("button", { name: "导入", exact: true }).click();
  await expect(dialog).toContainText(/导入\s*2/);
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(page.locator("[data-software-id]").filter({ hasText: "Notion" })).toHaveCount(1);
});

async function backupSettings(page: Page) {
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByRole("tab", { name: "备份", exact: true }).click();
  return dialog;
}

test("删除索引中的软件后，可以从备份恢复", async ({ page }) => {
  await page.goto("/#/library");
  const software = page.locator("[data-software-id]").filter({ hasText: "7-Zip" });
  await expect(software).toBeVisible();
  let settings = await backupSettings(page);
  await expect(settings.getByRole("button", { name: "恢复", exact: true })).toHaveCount(3);
  await settings.getByRole("button", { name: "立即备份" }).click();
  await expect(settings.getByRole("button", { name: "恢复", exact: true })).toHaveCount(4);
  await settings.getByRole("button", { name: "关闭", exact: true }).click();

  await software.click({ button: "right" });
  await page.getByRole("menuitem", { name: "删除软件", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "删除", exact: true }).click();
  await expect(software).toHaveCount(0);

  settings = await backupSettings(page);
  await settings.getByRole("button", { name: "恢复", exact: true }).first().click();
  await page.getByRole("dialog").filter({ hasText: "替换当前索引" }).getByRole("button", { name: "恢复", exact: true }).click();
  await settings.getByRole("button", { name: "关闭", exact: true }).click();
  await expect(software).toBeVisible();
});

test("侧栏和详情栏能恢复旧偏好并记住重新调整后的状态", async ({ page }) => {
  await page.addInitScript(() => {
    if (localStorage.getItem("shibapack.ui") === null) {
      localStorage.setItem("shibapack.ui", JSON.stringify({
        state: { sidebarCollapsed: true, detailOpen: false },
        version: 2,
      }));
    }
  });
  await page.goto("/#/library");
  const expand = page.getByRole("button", { name: "展开侧栏", exact: true });
  const collapse = page.getByRole("button", { name: "收起侧栏", exact: true });
  const showDetails = page.getByRole("button", { name: "显示详情栏", exact: true });
  const hideDetails = page.getByRole("button", { name: "隐藏详情栏", exact: true });
  await expect(expand).toBeVisible();
  await expect(showDetails).toBeVisible();

  await expand.click();
  await showDetails.click();
  await page.reload();
  await expect(collapse).toBeVisible();
  await expect(hideDetails).toBeVisible();

  await collapse.click();
  await hideDetails.click();
  await page.reload();
  await expect(expand).toBeVisible();
  await expect(showDetails).toBeVisible();
});

test("软件右键菜单可以更新图标，取不到时显示提示", async ({ page }) => {
  await page.goto("/#/library");
  const refresh = async (name: string) => {
    const software = page.locator("[data-software-id]").filter({ hasText: name });
    await software.click({ button: "right" });
    await page.getByRole("menuitem", { name: "更新图标", exact: true }).click();
    return software;
  };
  const blender = await refresh("Blender");
  await expect(blender.locator("img")).toBeVisible();
  await expect(page.getByText("已更新「Blender」的图标", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "网格", exact: true }).click();
  const other = await refresh("7-Zip");
  await expect(page.getByText("未找到「7-Zip」可用的图标，已保留原样", { exact: true })).toBeVisible();
  await expect(other.locator("img")).toHaveCount(0);
  await expect(blender.locator("img")).toBeVisible();
});

test("已切走仓库的后台简介不会覆盖当前仓库", async ({ page }) => {
  await page.goto("/#/library");
  await expect(page.locator("[data-software-id]").first()).toBeVisible();
  const result = await page.evaluate(async () => {
    const apiModule = new URL("/src/lib/api.ts", location.origin).href;
    const storeModule = new URL("/src/lib/store.ts", location.origin).href;
    const { api } = await import(apiModule) as typeof import("../../src/lib/api");
    const { useApp } = await import(storeModule) as typeof import("../../src/lib/store");
    const name = "Background job from the previous library";
    const revision = useApp.getState().status?.repoRevision;
    const old = useApp.getState().library!;
    const fetch = api.fetchSummary;
    let complete!: (snapshot: typeof old) => void;
    const response = new Promise<typeof old>((resolve) => { complete = resolve; });
    api.fetchSummary = (title, refresh, owner) => title === name ? response : fetch(title, refresh, owner);
    try {
      useApp.getState().requestSummary(name, true);
      await useApp.getState().chooseRepo("/different-library");
      complete({ ...old, summaries: { ...old.summaries, [name]: { text: "stale", source: "manual", fetchedAt: "" } } });
      await response;
      await new Promise((resolve) => setTimeout(resolve, 0));
      return { switched: useApp.getState().status?.repoRevision !== revision, stale: name in useApp.getState().library!.summaries };
    } finally {
      api.fetchSummary = fetch;
    }
  });
  expect(result).toEqual({ switched: true, stale: false });
});

test("深度校验可以取消，也可以从结果重新定位文件", async ({ page }) => {
  await page.goto("/#/library");
  await page.getByRole("button", { name: "设置", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByRole("tab", { name: "存储", exact: true }).click();
  await dialog.evaluate((element) => {
    const observer = new MutationObserver(() => {
      const cancel = [...element.querySelectorAll("button")].find((button) => button.textContent?.trim() === "取消" && !button.disabled);
      if (!cancel) return;
      observer.disconnect();
      cancel.click();
    });
    observer.observe(element, { subtree: true, childList: true, attributes: true, attributeFilter: ["disabled"] });
  });
  await dialog.getByRole("button", { name: "深度校验", exact: true }).click();
  await expect(page.getByText("已取消", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "开始校验", exact: true }).click();
  await expect(dialog.getByText("文件不存在", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "重新定位", exact: true }).click();
  await page.getByRole("menuitem", { name: "选择安装包文件…", exact: true }).click();
  await expect(dialog.getByText("文件不存在", { exact: true })).toHaveCount(0);
  await expect(page.getByText("已重新关联 1 个文件，跳过 0 个未匹配文件。", { exact: true })).toBeVisible();
});

test("大仓库列表只渲染可见行，滚到底部仍能用键盘选中", async ({ page }) => {
  await page.goto("/#/library");
  const rows = page.locator("main [data-software-id]");
  await expect(rows.first()).toBeVisible();
  await page.evaluate(async () => {
    const storeModule = new URL("/src/lib/store.ts", location.origin).href;
    const { useApp } = await import(storeModule) as typeof import("../../src/lib/store");
    const library = useApp.getState().library!;
    const sample = library.software[0];
    const software = Array.from({ length: 350 }, (_, i) => ({
      ...sample, id: `large-${i}`, name: `Package ${i}`,
    }));
    useApp.setState({ library: { ...library, software } });
  });
  await expect(page.locator('[data-software-id="large-0"]')).toBeVisible();
  expect(await rows.count()).toBeLessThan(50);
  await page.locator("main .overflow-y-auto").first().evaluate((element) => { element.scrollTop = element.scrollHeight; });
  const last = page.locator('[data-software-id="large-349"]');
  await expect(last).toBeInViewport();
  await last.click();
  await page.keyboard.press("ArrowUp");
  const previous = page.locator('[data-software-id="large-348"]');
  await expect(previous).toHaveAttribute("aria-pressed", "true");
  await expect(previous).toBeInViewport();
});

test("确认清理单个项目后显示结果并刷新列表", async ({ page }) => {
  await page.goto("/#/cleanup");
  const item = page.locator('[data-nav="item:system.temp"]');
  await expect(item).toBeVisible();
  await expect(page.getByRole("button", { name: "重新扫描", exact: true })).toBeEnabled();
  await item.click();
  await page.getByRole("button", { name: "清理这一项", exact: true }).click();
  await page.getByRole("dialog", { name: "清理 1 项" }).getByRole("button", { name: "清理", exact: true }).click();
  const result = page.getByRole("dialog", { name: "清理完成" });
  await expect(result.getByText(/清掉 [1-9]\d* 条/)).toBeVisible();
  await result.getByRole("button", { name: "关闭", exact: true }).last().click();
  await expect(item).toHaveCount(0);
});

test("已安装软件能在列表和网格按安装日期排序，未知日期始终在末尾", async ({ page }) => {
  await page.goto("/#/installed");
  const main = page.locator("main");
  const rows = main.locator('div[role="button"][aria-pressed]');
  const dateHeader = main.getByRole("button", { name: "安装日期", exact: true });
  const newestFirst = [
    /Microsoft Edge/, /DataGrip/, /Figma/, /Docker/, /Obsidian/,
    /Visual Studio Code/, /VLC/, /Safari/, /Calculator/,
  ];
  const oldestFirst = [...newestFirst.slice(0, -1).reverse(), /Calculator/];

  await dateHeader.click();
  await expect(rows).toHaveText(newestFirst);
  await dateHeader.click();
  await expect(rows).toHaveText(oldestFirst);
  await expect(rows.last()).toContainText("未知");

  await page.goto("/#/home");
  await page.goto("/#/installed");
  await expect(rows).toHaveText(oldestFirst);
  await page.reload();
  await expect(rows).toHaveText(oldestFirst);

  await page.setViewportSize({ width: 620, height: 760 });
  await main.getByRole("button", { name: "视图 · 列表", exact: true }).click();
  await page.getByRole("menuitemradio", { name: "网格", exact: true }).click();
  await expect(dateHeader).toHaveCount(0);
  const sort = main.getByRole("button", { name: "排序", exact: true });
  await sort.click();
  await expect(page.getByRole("menuitemcheckbox", { name: "安装日期", exact: true })).toBeChecked();
  await page.getByRole("menuitemcheckbox", { name: "降序", exact: true }).click();
  await page.keyboard.press("Escape");
  await expect(rows).toHaveText(newestFirst);
});

test("等待 Windows 卸载器完成后清理残留并刷新已安装列表", async ({ page }) => {
  await page.goto("/#/installed");
  const installed = page.locator("main").getByText("Microsoft Edge", { exact: true });
  await installed.click({ button: "right" });
  await page.getByRole("menuitem", { name: "卸载", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("安装目录", { exact: true })).toBeVisible();
  await expect(dialog.getByText("卸载命令", { exact: true })).toBeVisible();
  await dialog.getByRole("button", { name: "卸载", exact: true }).click();
  await expect(dialog.getByText("等待卸载程序…", { exact: true })).toBeVisible();
  await expect(installed).toHaveCount(1);
  await expect(installed).toHaveCount(0);
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(dialog).toHaveCount(0);
});

test("多选模式可以从工具栏批量卸载并保留其他软件", async ({ page }) => {
  await page.goto("/#/installed");
  const list = page.locator("main");
  const edge = list.getByText("Microsoft Edge", { exact: true });
  const vlc = list.getByText("VLC", { exact: true });
  await list.getByRole("button", { name: "多选", exact: true }).click();
  await edge.click();
  await vlc.click();
  await list.getByRole("button", { name: "卸载 2 个", exact: true }).click();

  const dialog = page.getByRole("dialog", { name: "卸载 2 个软件" });
  await expect(dialog.getByText(/Microsoft Edge/)).toBeVisible();
  await expect(dialog.getByText(/VLC/)).toBeVisible();
  await dialog.getByRole("button", { name: "卸载 2 个", exact: true }).click();
  await expect(dialog.getByText("等待卸载程序…", { exact: true })).toBeVisible();
  await expect(vlc).toHaveCount(1);
  await expect(edge).toHaveCount(0);
  await expect(vlc).toHaveCount(0);
  await expect(list.getByText("DataGrip", { exact: true })).toHaveCount(1);
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(dialog).toHaveCount(0);
});

test("卸载器尚未完成时手动继续，软件仍留在已安装列表", async ({ page }) => {
  await page.goto("/#/installed");
  const edge = page.locator("main").getByText("Microsoft Edge", { exact: true });
  await edge.click({ button: "right" });
  await page.getByRole("menuitem", { name: "卸载", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "卸载", exact: true }).click();
  await dialog.getByRole("button", { name: "已经卸载完了", exact: true }).click();
  await dialog.getByRole("button", { name: "完成", exact: true }).click();
  await expect(edge).toHaveCount(1);
});
