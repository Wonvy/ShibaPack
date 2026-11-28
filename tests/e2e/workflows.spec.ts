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
