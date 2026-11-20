// 导入窗口：把文件拖进中间栏后先在这里看一眼要导入什么、放进哪个分类、怎么存，确认后才导入。
// 导入时窗口不关，显示进度；导完显示结果，失败的文件逐个说清楚在哪一步、为什么。
// 不是从这里开始的导入（工具栏、拖到侧栏的分类上）有失败时，也用它显示结果。

import { AlertCircle, AppWindow, CheckCircle2, Copy, FileBox, Folder, LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { errorText, importFailureText } from "../lib/i18n";
import { categoryPath, flattenTree, formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { ImportDialogState } from "../lib/store";
import type { DropEntry, ImportItem, ImportReport, Storage } from "../lib/types";
import { Button, MiddleTruncate, Modal, Segmented, Select, cx } from "./ui";

type CategoryMode = "current" | "smart" | "pick" | "none";
type StorageMode = "copy" | "move" | "index";
type Phase = "confirm" | "running" | "done";

export function ImportDialog() {
  const state = useApp((s) => s.importDialog);
  if (!state) return null;
  // 换了一批文件就从头来
  return <Dialog key={state.paths.join("\n") + (state.report ? "#report" : "")} state={state} />;
}

function Dialog({ state }: { state: ImportDialogState }) {
  const t = useT();
  const language = useLanguage();
  const library = useApp((s) => s.library);
  const scope = useApp((s) => s.scope);
  const route = useApp((s) => s.route);
  const patch = useApp((s) => s.patch);
  const importPaths = useApp((s) => s.importPaths);
  const cancelImport = useApp((s) => s.cancelImport);
  const progress = useApp((s) => s.progress);

  const categories = library?.categories ?? [];
  // 正看着某个分类时，默认放进这个分类
  const current = route === "library" && scope.kind === "category" && categories.some((c) => c.id === scope.id) ? scope.id : null;
  const preset = state.categoryId && categories.some((c) => c.id === state.categoryId) ? state.categoryId : null;

  const [entries, setEntries] = useState<DropEntry[] | null>(state.entries ?? null);
  const [inspectError, setInspectError] = useState<string | null>(null);
  /** 整个当作一个软件导入的文件夹。 */
  const [packages, setPackages] = useState<Set<string>>(() => defaultPackages(state.entries ?? []));
  const [categoryMode, setCategoryMode] = useState<CategoryMode>(preset ? "pick" : state.categoryId !== null && current ? "current" : "none");
  const [picked, setPicked] = useState<string>(preset ?? current ?? categories[0]?.id ?? "");
  const settings = library?.settings;
  const [storage, setStorage] = useState<StorageMode>(
    settings?.defaultStorage === "indexed" ? "index" : settings?.moveOnImport ? "move" : "copy",
  );
  const [phase, setPhase] = useState<Phase>(state.report ? "done" : "confirm");
  const [report, setReport] = useState<ImportReport | null>(state.report ?? null);
  const [runError, setRunError] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  /** 这次导入用了智能分类：导完给一个按钮接着分类。 */
  const [smart, setSmart] = useState(false);

  useEffect(() => {
    if (entries !== null) return;
    let alive = true;
    api.inspectDrop(state.paths).then(
      (found) => {
        if (!alive) return;
        setEntries(found);
        setPackages(defaultPackages(found));
      },
      (error) => alive && setInspectError(errorText(language, error)),
    );
    return () => {
      alive = false;
    };
    // 只在打开时看一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const close = () => patch({ importDialog: null });
  const tree = useMemo(() => flattenTree(categories), [categories]);

  const start = async () => {
    const categoryId = categoryMode === "current" ? current : categoryMode === "pick" ? picked || null : null;
    const mode: Storage = storage === "index" ? "indexed" : "managed";
    setSmart(categoryMode === "smart");
    setPhase("running");
    setStopping(false);
    try {
      const result = await importPaths(
        state.paths,
        mode,
        { categoryId: categoryId ?? undefined, packages: [...packages] },
        { move: storage === "move", quiet: true },
      );
      const done = result ?? { imported: 0, duplicates: 0, failed: 0, items: [], cancelled: true };
      setReport(done);
      // 导入时点了“在后台继续”，窗口已经关了：有失败的重新打开显示原因，都成功就提示一句
      const app = useApp.getState();
      if (app.importDialog !== state) {
        if (done.failed > 0 && app.importDialog === null) app.patch({ importDialog: { paths: state.paths, report: done } });
        else app.notify(t("import.done", { imported: done.imported, duplicates: done.duplicates, failed: done.failed }), done.failed > 0 ? "error" : "info");
      }
    } catch (error) {
      setRunError(errorText(language, error));
      if (useApp.getState().importDialog !== state) useApp.getState().notify(errorText(language, error), "error");
    }
    setPhase("done");
  };

  // 导入的软件（去掉重复的 id）：只导入了一个时“完成”顺手选中它，智能分类也只分这些
  const importedIds = useMemo(
    () => [...new Set((report?.items ?? []).filter((i) => i.status === "imported" && i.softwareId).map((i) => i.softwareId!))],
    [report],
  );
  const finish = () => {
    close();
    if (importedIds.length === 1) {
      const app = useApp.getState();
      app.setRoute("library");
      app.select(importedIds[0]);
    }
  };
  const classify = () => patch({ importDialog: null, classifyIds: importedIds });

  const title =
    phase === "confirm" ? t("importDialog.title", { n: state.paths.length }) : phase === "running" ? t("importDialog.running") : t("importDialog.doneTitle");

  const footer =
    phase === "confirm" ? (
      <>
        <Button variant="ghost" onClick={close}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={entries === null || !library} onClick={() => void start()}>
          {t("importDialog.start")}
        </Button>
      </>
    ) : phase === "running" ? (
      <>
        {/* 关掉窗口也照常导入，进度在底部状态栏 */}
        <Button variant="ghost" onClick={close}>
          {t("importDialog.background")}
        </Button>
        <Button
          disabled={stopping}
          onClick={() => {
            setStopping(true);
            cancelImport();
          }}
        >
          {stopping ? t("importDialog.stopping") : t("importDialog.stop")}
        </Button>
      </>
    ) : (
      <>
        {smart && importedIds.length > 0 && (
          <Button variant="ghost" onClick={finish}>
            {t("importDialog.done")}
          </Button>
        )}
        {smart && importedIds.length > 0 ? (
          <Button variant="primary" onClick={classify}>
            {t("importDialog.classify", { n: importedIds.length })}
          </Button>
        ) : (
          <Button variant="primary" onClick={finish}>
            {t("importDialog.done")}
          </Button>
        )}
      </>
    );

  return (
    <Modal open onClose={close} title={title} width={540} footer={footer}>
      {phase === "confirm" && (
        <div className="flex flex-col gap-4">
          <EntryList
            entries={entries}
            error={inspectError}
            packages={packages}
            onPackage={(path, whole) => {
              const next = new Set(packages);
              if (whole) next.add(path);
              else next.delete(path);
              setPackages(next);
            }}
          />
          <section>
            <div className="mb-1.5 flex items-center justify-between gap-3">
              <span className="text-[12px] font-medium text-fg-2">{t("importDialog.category")}</span>
              <Segmented<CategoryMode>
                value={categoryMode}
                onChange={setCategoryMode}
                options={[
                  ...(current ? [{ value: "current" as const, label: t("importDialog.cat.current") }] : []),
                  { value: "smart", label: t("importDialog.cat.smart") },
                  ...(categories.length > 0 ? [{ value: "pick" as const, label: t("importDialog.cat.pick") }] : []),
                  { value: "none", label: t("importDialog.cat.none") },
                ]}
              />
            </div>
            {categoryMode === "pick" ? (
              <div className="flex items-center gap-2">
                <Select value={picked} onChange={(event) => setPicked(event.target.value)} className="min-w-0 flex-1">
                  {tree.map(({ category, depth }) => (
                    <option key={category.id} value={category.id}>
                      {"　".repeat(depth) + category.name}
                    </option>
                  ))}
                </Select>
              </div>
            ) : null}
            <p className="m-0 mt-1.5 text-[12px] leading-relaxed text-fg-3">
              {categoryMode === "current" && current
                ? t("importDialog.cat.currentHint", { name: categoryPath(categories, current) })
                : categoryMode === "smart"
                  ? t("importDialog.cat.smartHint")
                  : categoryMode === "pick"
                    ? t("importDialog.cat.pickHint")
                    : t("importDialog.cat.noneHint")}
            </p>
          </section>
          <section>
            <div className="mb-1.5 flex items-center justify-between gap-3">
              <span className="text-[12px] font-medium text-fg-2">{t("importDialog.storage")}</span>
              <Segmented<StorageMode>
                value={storage}
                onChange={setStorage}
                options={[
                  { value: "copy", label: t("importDialog.copy") },
                  { value: "move", label: t("importDialog.move") },
                  { value: "index", label: t("importDialog.index") },
                ]}
              />
            </div>
            <p className="m-0 text-[12px] leading-relaxed text-fg-3">
              {storage === "copy" ? t("importDialog.copyHint") : storage === "move" ? t("importDialog.moveHint") : t("importDialog.indexHint")}
            </p>
          </section>
        </div>
      )}

      {phase === "running" && <Running progress={progress} stopping={stopping} />}

      {phase === "done" && (runError ? <p className="m-0 text-[13px] text-danger">{runError}</p> : report && <Results report={report} />)}
    </Modal>
  );
}

/** 文件夹默认怎么导：像一个软件（绿色版、.app）或者里面没有安装包时整个导入，否则导入里面的安装包。 */
function defaultPackages(entries: DropEntry[]): Set<string> {
  return new Set(entries.filter((e) => e.isDir && (e.package || e.installers === 0)).map((e) => e.path));
}

function EntryList({
  entries,
  error,
  packages,
  onPackage,
}: {
  entries: DropEntry[] | null;
  error: string | null;
  packages: Set<string>;
  onPackage: (path: string, whole: boolean) => void;
}) {
  const t = useT();
  if (error) return <p className="m-0 text-[13px] text-danger">{error}</p>;
  if (entries === null) {
    return (
      <div className="flex h-16 items-center justify-center gap-2 rounded-lg border border-line text-[13px] text-fg-3">
        <LoaderCircle size={15} strokeWidth={2} className="animate-spin" aria-hidden />
        {t("importDialog.inspecting")}
      </div>
    );
  }
  return (
    <div className="max-h-[38vh] overflow-y-auto rounded-lg border border-line">
      {entries.map((entry) => {
        const whole = packages.has(entry.path);
        const app = entry.isDir && entry.name.toLowerCase().endsWith(".app");
        const Icon = app ? AppWindow : entry.isDir ? Folder : FileBox;
        const guess = [entry.guessName, entry.guessVersion].filter(Boolean).join(" ");
        return (
          <div key={entry.path} className="flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0">
            <Icon size={18} strokeWidth={1.5} aria-hidden className="shrink-0 text-fg-3" />
            <div className="min-w-0 flex-1">
              <MiddleTruncate text={entry.name} title={entry.path} className="text-[13px] text-fg" />
              <div className="mt-0.5 flex min-w-0 items-center gap-2 text-[12px] text-fg-3">
                {entry.isDir ? (
                  whole || app ? (
                    <span className="truncate">{guess ? `→ ${guess}` : t("importDialog.wholeHint")}</span>
                  ) : (
                    <span className="truncate">{t("importDialog.insideHint", { n: entry.installers })}</span>
                  )
                ) : (
                  <>
                    {entry.size !== null && <span className="shrink-0 tabular-nums">{formatSize(entry.size)}</span>}
                    <span className="truncate">{entry.installer ? (guess ? `→ ${guess}` : "") : t("importDialog.notInstaller")}</span>
                  </>
                )}
                {entry.isDir && entry.package && !app && (
                  <span className="shrink-0 rounded bg-hover px-1.5 py-px text-[12px] text-fg-2">{t("importDialog.looksLikeApp")}</span>
                )}
              </div>
            </div>
            {/* .app 一定整个导入；里面没有安装包的文件夹也只能整个导入 */}
            {entry.isDir && !app && entry.installers > 0 && (
              <Segmented<"whole" | "inside">
                value={whole ? "whole" : "inside"}
                onChange={(value) => onPackage(entry.path, value === "whole")}
                options={[
                  { value: "whole", label: t("importDialog.asPackage") },
                  { value: "inside", label: t("importDialog.inside", { n: entry.installers }) },
                ]}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

function Running({ progress, stopping }: { progress: ReturnType<typeof useApp.getState>["progress"]; stopping: boolean }) {
  const t = useT();
  const queued = progress === null;
  const known = progress?.phase === "import" && progress.total > 0;
  const percent = known ? Math.round((progress.current / progress.total) * 100) : 0;
  return (
    <div className="flex flex-col gap-2 py-2">
      <div className="flex items-baseline justify-between gap-3 text-[13px]">
        <span className="min-w-0 truncate text-fg-2">
          {stopping
            ? t("importDialog.stopping")
            : queued
              ? t("importDialog.queued")
              : known
                ? progress.file
                : t("importDialog.collecting")}
        </span>
        {known && (
          <span className="shrink-0 text-[12px] text-fg-3 tabular-nums">
            {t("importDialog.progress", { current: progress.current, total: progress.total })}
          </span>
        )}
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-line">
        {known ? (
          <div className="h-full rounded-full bg-accent transition-[width] duration-200" style={{ width: `${percent}%` }} />
        ) : (
          <div className="h-full w-2/5 animate-slide rounded-full bg-accent" />
        )}
      </div>
    </div>
  );
}

function Results({ report }: { report: ImportReport }) {
  const t = useT();
  const language = useLanguage();
  const failed = report.items.filter((i) => i.status === "failed");
  const duplicates = report.items.filter((i) => i.status === "duplicate");
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-3 gap-2">
        <Stat label={t("importDialog.imported")} value={report.imported} tone={report.imported > 0 ? "ok" : undefined} />
        <Stat label={t("importDialog.duplicates")} value={report.duplicates} />
        <Stat label={t("importDialog.failed")} value={report.failed} tone={report.failed > 0 ? "bad" : undefined} />
      </div>
      {report.cancelled && <p className="m-0 text-[12px] text-fg-3">{t("importDialog.cancelledNote")}</p>}
      {report.items.length === 0 && !report.cancelled && <p className="m-0 text-[13px] text-fg-3">{t("importDialog.nothing")}</p>}
      {failed.length > 0 && (
        <section>
          <h3 className="m-0 mb-1.5 text-[12px] font-medium text-fg-2">{t("importDialog.failures")}</h3>
          <div className="max-h-[40vh] overflow-y-auto rounded-lg border border-line">
            {failed.map((item) => (
              <FailureRow key={item.path} item={item} language={language} />
            ))}
          </div>
          <p className="m-0 mt-1.5 text-[12px] text-fg-3">{t("importDialog.logHint")}</p>
        </section>
      )}
      {duplicates.length > 0 && (
        <section>
          <h3 className="m-0 mb-1.5 text-[12px] font-medium text-fg-2">{t("importDialog.duplicateList")}</h3>
          <div className="max-h-[20vh] overflow-y-auto rounded-lg border border-line">
            {duplicates.map((item) => (
              <div key={item.path} className="flex items-center gap-2 border-b border-line px-3 py-1.5 text-[12px] text-fg-2 last:border-b-0">
                <Copy size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
                <MiddleTruncate text={baseName(item.path)} title={item.path} className="min-w-0 flex-1" />
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function FailureRow({ item, language }: { item: ImportItem; language: ReturnType<typeof useLanguage> }) {
  const { step, reason, raw } = importFailureText(language, item);
  return (
    <div className="flex gap-2.5 border-b border-line px-3 py-2 last:border-b-0">
      <AlertCircle size={15} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0 text-danger" />
      <div className="min-w-0 flex-1">
        <MiddleTruncate text={baseName(item.path)} title={item.path} className="text-[13px] text-fg" />
        <p className="m-0 mt-0.5 text-[12px] leading-relaxed text-fg-2">{step ? `${step}：${reason}` : reason}</p>
        {raw && raw !== reason && <p className="m-0 mt-0.5 text-[12px] break-all text-fg-3 select-text">{raw}</p>}
        <p className="m-0 mt-0.5 truncate text-[12px] text-fg-3 select-text" title={item.path}>
          {item.path}
        </p>
      </div>
    </div>
  );
}

function Stat({ label, value, tone }: { label: string; value: number; tone?: "ok" | "bad" }) {
  return (
    <div className="flex items-center gap-2 rounded-lg border border-line px-3 py-2">
      {tone === "ok" && <CheckCircle2 size={15} strokeWidth={1.75} aria-hidden className="shrink-0 text-accent" />}
      {tone === "bad" && <AlertCircle size={15} strokeWidth={1.75} aria-hidden className="shrink-0 text-danger" />}
      <span className="min-w-0 flex-1 truncate text-[12px] text-fg-3">{label}</span>
      <span className={cx("text-[15px] font-semibold tabular-nums", tone === "bad" ? "text-danger" : "text-fg")}>{value}</span>
    </div>
  );
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
