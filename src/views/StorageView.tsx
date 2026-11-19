// 存储与备份：仓库位置、统计、回收站、备份列表与恢复、文件校验、索引导出导入。

import { CircleCheck, TriangleAlert } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { confirm } from "../components/dialogs";
import { Button, Select } from "../components/ui";
import { deleteVersions } from "../lib/actions";
import { api, pickFolder, pickJsonFile, pickSavePath } from "../lib/api";
import { formatDate, formatSize, prunable } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { BackupInfo, MissingFile, Stats, VerifyReport } from "../lib/types";

export function StorageView() {
  const t = useT();
  const language = useLanguage();
  const status = useApp((state) => state.status);
  const library = useApp((state) => state.library);
  const apply = useApp((state) => state.apply);
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const chooseRepo = useApp((state) => state.chooseRepo);
  const tidyRoot = useApp((state) => state.tidyRoot);
  const links = useApp((state) => state.links);
  const installed = useApp((state) => state.installed);
  const [tidying, setTidying] = useState(false);
  const [keep, setKeep] = useState(2);

  // 每个软件只留最新 keep 个版本时要删的版本；本机装着的版本留着
  const pruneTargets = useMemo(
    () =>
      (library?.software ?? []).flatMap((software) => {
        const local = installed?.find((app) => app.id === links[software.id])?.version;
        return prunable(software, keep, local).map((version) => ({ software, version }));
      }),
    [library, links, installed, keep],
  );
  const pruneApps = new Set(pruneTargets.map((target) => target.software.id)).size;
  const pruneSize = pruneTargets.reduce((sum, { version }) => sum + version.files.reduce((n, f) => n + f.size, 0), 0);
  const pruneSummary = t("storage.pruneSummary", { m: pruneApps, n: pruneTargets.length, size: formatSize(pruneSize) });

  const pruneAll = async () => {
    const lang = { m: pruneApps, n: pruneTargets.length };
    const { ok, checked } = await confirm({
      title: t("storage.pruneConfirm", lang),
      body: pruneSummary,
      confirmLabel: t("prune.confirm", { n: pruneTargets.length }),
      danger: true,
      checkbox: { label: t("del.alsoDisk"), hintOn: t("del.diskHint"), hintOff: t("del.recordOnly"), initial: true },
    });
    if (ok) await deleteVersions(pruneTargets, checked);
  };

  const [stats, setStats] = useState<Stats | null>(null);
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [report, setReport] = useState<VerifyReport | null>(null);
  const [verifying, setVerifying] = useState(false);

  // 索引一变（导入、删除、恢复）就重新取统计和备份列表
  useEffect(() => {
    let alive = true;
    void Promise.all([api.getStats(), api.listBackups()]).then(
      ([nextStats, nextBackups]) => {
        if (!alive) return;
        setStats(nextStats);
        setBackups(nextBackups);
      },
      () => {},
    );
    return () => {
      alive = false;
    };
  }, [library, status?.repoRoot]);

  const changeRepo = async () => {
    const folder = await pickFolder();
    if (folder) {
      setReport(null);
      await chooseRepo(folder);
    }
  };

  const backupNow = async () => {
    const list = await attempt(() => api.createBackup());
    if (list) setBackups(list);
  };

  const restore = async (backup: BackupInfo) => {
    const { ok } = await confirm({
      title: t("storage.restore"),
      body: t("storage.restoreConfirm", { name: formatDate(backup.createdAt, language, true) }),
      confirmLabel: t("storage.restore"),
    });
    if (ok) await apply(() => api.restoreBackup(backup.name));
  };

  const emptyTrash = async () => {
    const { ok } = await confirm({
      title: t("storage.emptyTrash"),
      body: t("storage.emptyTrashConfirm"),
      confirmLabel: t("storage.emptyTrash"),
      danger: true,
    });
    if (!ok) return;
    const next = await attempt(() => api.emptyTrash());
    if (next) setStats(next);
  };

  const verify = async () => {
    setVerifying(true);
    const next = await attempt(() => api.verifyData());
    setVerifying(false);
    if (next) setReport(next);
  };

  const dropRecord = async (problem: MissingFile) => {
    const ref = { softwareId: problem.softwareId, versionId: problem.versionId, fileId: problem.fileId };
    if (await apply(() => api.deleteFile(ref, false))) {
      setReport((current) => current && { ...current, problems: current.problems.filter((p) => p.fileId !== problem.fileId) });
    }
  };

  const tidy = async () => {
    setTidying(true);
    await tidyRoot();
    setTidying(false);
  };

  const exportIndex = async () => {
    const dest = await pickSavePath("software.json");
    if (dest && (await attempt(() => api.exportBackup(dest).then(() => true)))) notify(t("storage.exported"));
  };

  const importIndex = async () => {
    const src = await pickJsonFile();
    if (!src) return;
    const { ok } = await confirm({ title: t("storage.import"), body: t("storage.importConfirm"), confirmLabel: t("common.confirm") });
    if (ok) await apply(() => api.importBackup(src));
  };

  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto max-w-[720px] px-6 pt-1 pb-10">
        <Group title={t("storage.repo")} hint={t("storage.repoHint")}>
          <div className="flex items-center gap-2">
            <div className="selectable min-w-0 flex-1 truncate rounded-md bg-hover px-2.5 py-1.5 font-mono text-[12px] text-fg-2">
              {status?.repoRoot}
            </div>
            <Button onClick={() => void attempt(() => api.openRepoFolder())}>{t("storage.open")}</Button>
            <Button onClick={() => void changeRepo()}>{t("storage.change")}</Button>
          </div>
        </Group>

        <Group title={t("storage.stats")}>
          <div className="grid grid-cols-3 gap-x-6 gap-y-4 sm:grid-cols-6">
            <Stat label={t("storage.software")} value={stats?.software} />
            <Stat label={t("storage.versions")} value={stats?.versions} />
            <Stat label={t("storage.files")} value={stats?.files} />
            <Stat label={t("storage.total")} value={stats && formatSize(stats.totalSize)} />
            <Stat label={t("storage.managed")} value={stats && formatSize(stats.managedSize)} />
            <Stat label={t("storage.indexed")} value={stats && formatSize(stats.indexedSize)} />
          </div>
        </Group>

        <Group
          title={t("storage.prune")}
          hint={t("storage.pruneHint")}
          action={
            <Button disabled={pruneTargets.length === 0} onClick={() => void pruneAll()}>
              {t("storage.pruneRun")}
            </Button>
          }
        >
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-[13px]">
            <label className="flex items-center gap-2 text-fg-2">
              {t("storage.pruneKeep")}
              <Select compact className="w-auto" value={String(keep)} onChange={(event) => setKeep(Number(event.target.value))}>
                {[1, 2, 3, 5].map((n) => (
                  <option key={n} value={n}>
                    {t("lib.versions", { n })}
                  </option>
                ))}
              </Select>
            </label>
            <span className={pruneTargets.length > 0 ? "text-fg" : "text-fg-3"}>
              {pruneTargets.length > 0 ? pruneSummary : t("storage.pruneNone")}
            </span>
          </div>
        </Group>

        <Group
          title={t("storage.backups")}
          hint={t("storage.backupHint")}
          action={<Button onClick={() => void backupNow()}>{t("storage.backupNow")}</Button>}
        >
          {backups.length === 0 ? (
            <div className="text-[12px] text-fg-4">{t("storage.noBackups")}</div>
          ) : (
            backups.map((backup) => (
              <div key={backup.name} className="flex h-9 items-center gap-3 border-b border-line last:border-b-0">
                <span className="tabular text-[13px]">{formatDate(backup.createdAt, language, true)}</span>
                <span className="tabular text-[12px] text-fg-3">{formatSize(backup.size)}</span>
                <Button variant="ghost" className="ml-auto" onClick={() => void restore(backup)}>
                  {t("storage.restore")}
                </Button>
              </div>
            ))
          )}
        </Group>

        <Group
          title={t("storage.verify")}
          hint={t("storage.verifyHint")}
          action={
            <Button busy={verifying} onClick={() => void verify()}>
              {t("storage.verifyRun")}
            </Button>
          }
        >
          {report && report.problems.length === 0 && (
            <div className="flex items-center gap-2 text-[13px] text-success">
              <CircleCheck size={15} strokeWidth={1.75} aria-hidden />
              {t("storage.verifyOk", { n: report.checked })}
            </div>
          )}
          {report && report.problems.length > 0 && (
            <>
              <div className="mb-1 flex items-center gap-2 text-[13px] text-warning">
                <TriangleAlert size={15} strokeWidth={1.75} aria-hidden />
                {t("storage.verifyProblems", { n: report.checked, m: report.problems.length })}
              </div>
              {report.problems.map((problem) => (
                <div key={problem.fileId} className="flex items-center gap-3 border-b border-line py-2 last:border-b-0">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13px]">
                      {problem.software}
                      {problem.version && <span className="text-fg-3"> · {problem.version}</span>}
                    </div>
                    <div className="selectable truncate font-mono text-[12px] text-fg-3" title={problem.path}>
                      {problem.path}
                    </div>
                  </div>
                  <span className="shrink-0 text-[12px] text-warning">{t(`storage.reason.${problem.reason}`)}</span>
                  <Button variant="ghost" onClick={() => void dropRecord(problem)}>
                    {t("storage.removeRecord")}
                  </Button>
                </div>
              ))}
            </>
          )}
        </Group>

        <Group
          title={t("storage.tidy")}
          hint={t("storage.tidyHint")}
          action={
            <Button busy={tidying} onClick={() => void tidy()}>
              {t("storage.tidyRun")}
            </Button>
          }
        />

        <Group
          title={t("storage.trash")}
          action={
            // 清空后无法恢复，用红字和别的按钮区分开
            <Button variant="caution" disabled={!stats || stats.trashSize === 0} onClick={() => void emptyTrash()}>
              {t("storage.emptyTrash")}
            </Button>
          }
        >
          <div className="tabular text-[13px] text-fg-2">{stats ? formatSize(stats.trashSize) : "…"}</div>
        </Group>

        <Group title={t("storage.transfer")} hint={t("storage.transferHint")}>
          <div className="flex gap-2">
            <Button onClick={() => void exportIndex()}>{t("storage.export")}</Button>
            <Button onClick={() => void importIndex()}>{t("storage.import")}</Button>
          </div>
        </Group>
      </div>
    </div>
  );
}

/** 一组设置：标题加一条下划线，不套卡片。 */
function Group({ title, hint, action, children }: { title: string; hint?: string; action?: ReactNode; children?: ReactNode }) {
  return (
    <section className="mt-7">
      <div className="mb-3 flex min-h-8 items-end justify-between gap-4 border-b border-line pb-2">
        <div>
          <h3 className="m-0 text-[13px] font-semibold">{title}</h3>
          {hint && <p className="m-0 mt-0.5 text-[12px] text-fg-3">{hint}</p>}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function Stat({ label, value }: { label: string; value: number | string | null | undefined }) {
  return (
    <div>
      <div className="text-[12px] text-fg-3">{label}</div>
      <div className="tabular mt-0.5 text-[17px] font-semibold">{value ?? "…"}</div>
    </div>
  );
}
