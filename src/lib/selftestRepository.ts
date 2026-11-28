import { api } from "./api";
import { translate, type Key } from "./i18n";
import { useApp } from "./store";

type Check = (name: string, ok: boolean) => Promise<void>;

async function waitFor<T>(read: () => T | undefined, description: string): Promise<T> {
  const deadline = performance.now() + 10_000;
  while (performance.now() < deadline) {
    const value = read();
    if (value !== undefined) return value;
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
  throw new Error("等待超时：" + description);
}

export async function runRepositoryWorkflows(source: string, check: Check) {
  const app = useApp.getState();
  if (!app.library || app.library.software.length !== 0) throw new Error("仓库自检需要一个空白的临时仓库");
  const t = (key: Key) => translate(app.library!.settings.language, key);
  const button = (key: Key) => waitFor(
    () => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button'))
      .find((candidate) => !candidate.disabled && candidate.textContent?.trim() === t(key)),
    t(key),
  );

  app.setRoute("library");
  app.patch({ importDialog: { paths: [source] } });
  (await button("importDialog.start")).click();
  // 等真实桌面后端开始处理单个文件，再点界面上的停止按钮。
  await waitFor(() => useApp.getState().progress?.phase === "import" ? true : undefined, "开始处理大文件");
  (await button("importDialog.stop")).click();
  await waitFor(() => document.querySelector('[role="dialog"]')?.textContent?.includes(t("importDialog.cancelledNote")) ? true : undefined, "取消结果");
  await check("停止大文件导入后，仓库索引仍然为空", (await api.getLibrary()).software.length === 0);
  (await button("importDialog.done")).click();

  const imported = await api.importPaths([source], "indexed", { autoClassify: false });
  const software = imported.library.software[0];
  await check("取消后仍可重新导入同一源文件", imported.report.imported === 1 && imported.report.failed === 0);
  if (!software) return;
  const [backup] = await api.createBackup();
  await check("只删除软件记录", (await api.deleteSoftware(software.id, false)).software.length === 0);
  const restored = await api.restoreBackup(backup.name);
  const verify = await api.verifyData();
  await check("从真实索引备份恢复软件后文件可用", restored.software.some((entry) => entry.id === software.id) && verify.problems.length === 0);
}
