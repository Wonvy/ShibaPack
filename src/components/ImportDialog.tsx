// 导入窗口：把文件拖进中间栏后先在这里看一眼要导入什么、放进哪个分类、怎么存，确认后才导入。
// 导入时窗口不关，显示进度；导完显示结果，失败的文件逐个说清楚在哪一步、为什么。
// 直接拖到侧栏分类的导入失败时，也用这个窗口显示结果。

import { AlertCircle, AppWindow, CheckCircle2, Copy, FileBox, Folder, Link2, LoaderCircle, TriangleAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { errorText, importFailureText } from "../lib/i18n";
import { categoryPath, flattenTree, formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { ImportDialogState } from "../lib/store";
import type { DropEntry, ImportCategoryPreview, ImportEntryEdit, ImportItem, ImportReport, Software, Storage } from "../lib/types";
import { SoftwareIcon } from "./SoftwareIcon";
import { Button, MiddleTruncate, Modal, Select, cx } from "./ui";

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

  const [entries, setEntries] = useState<DropEntry[] | null>(
    state.entries ?? null,
  );
  const [inspectError, setInspectError] = useState<string | null>(null);
  /** 整个当作一个软件导入的文件夹。 */
  const [packages, setPackages] = useState<Set<string>>(() => defaultPackages(state.entries ?? []));
  const settings = library?.settings;
  const [category, setCategory] = useState(
    preset ? `category:${preset}` : state.categoryId === null ? "none" : settings?.autoClassifyOnImport ? "smart" : current ? "current" : "none",
  );
  const [storage, setStorage] = useState<StorageMode>(
    (state.storage ?? settings?.defaultStorage) === "indexed" ? "index" : !state.storage && settings?.moveOnImport ? "move" : "copy",
  );
  const [phase, setPhase] = useState<Phase>(state.report ? "done" : "confirm");
  const [report, setReport] = useState<ImportReport | null>(state.report ?? null);
  const [runError, setRunError] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const [drafts, setDrafts] = useState<Record<string, Partial<ImportEntryEdit>>>({});
  const [previews, setPreviews] = useState(new Map<string, { name: string; category: ImportCategoryPreview | null }>());
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewAttempt, setPreviewAttempt] = useState(0);

  useEffect(() => {
    if (entries !== null || state.report) return;
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
  const existingSoftware = useMemo(
    () => new Map(library?.software.map((software) => [software.id, software])),
    [library?.software],
  );
  const editOf = useCallback((entry: DropEntry): ImportEntryEdit => {
    const matchedName = entry.matchedSoftwareId ? existingSoftware.get(entry.matchedSoftwareId)?.name : undefined;
    return {
      name: state.softwareName ?? drafts[entry.path]?.name ?? matchedName ?? entry.guessName,
      version: drafts[entry.path]?.version ?? entry.guessVersion,
    };
  }, [drafts, existingSoftware, state.softwareName]);
  const updateDraft = (path: string, edit: Partial<ImportEntryEdit>) =>
    setDrafts((current) => ({ ...current, [path]: { ...current[path], ...edit } }));
  const activeEntries = useMemo(() => entries ?? [], [entries]);
  const smart = category === "smart";
  const categoryTargets = useMemo(
    () => activeEntries.map((entry) => ({ path: entry.path, name: editOf(entry).name.trim(), publisher: entry.publisher })),
    [activeEntries, editOf],
  );
  useEffect(() => {
    if (!smart || phase !== "confirm" || categoryTargets.length === 0) return;
    let alive = true;
    setPreviews(new Map());
    setPreviewError(null);
    const update = (suggestions: ImportCategoryPreview[]) => {
      if (!alive) return;
      setPreviews(new Map(categoryTargets.map(({ path, name }, index) => [path, { name, category: suggestions[index] ?? null }])));
    };
    const timer = setTimeout(() => {
      void api.previewImportCategories(categoryTargets).then(update, (error) => {
        if (alive) setPreviewError(errorText(language, error));
        update([]);
      });
    }, 150);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [categoryTargets, phase, smart, library?.categories, language, previewAttempt]);

  const previewsReady = categoryTargets.every(({ path, name }) => {
    const preview = previews.get(path);
    return preview?.name === name && preview.category !== null;
  });

  const categoryOf = (entry: DropEntry, name: string): string | undefined => {
    if (!smart) return undefined;
    if (!name.trim()) return "—";
    const value = previews.get(entry.path);
    if (!value || value.name !== name.trim()) return t("importDialog.categoryLoading");
    if (!value.category) return t("importDialog.categoryUnavailable");
    const { categoryId, newCategory } = value.category;
    if (categoryId) return categoryPath(categories, categoryId);
    return newCategory?.name ?? t("nav.uncategorized");
  };
  const chosenCategory = category === "current" ? current : category.startsWith("category:") ? category.slice("category:".length) : null;
  const storageLabel = { copy: t("importDialog.copy"), move: t("importDialog.move"), index: t("importDialog.index") };
  const storageHint = { copy: t("importDialog.copyHint"), move: t("importDialog.moveHint"), index: t("importDialog.indexHint") };
  const categoryHint = chosenCategory
    ? t("importDialog.cat.currentHint", { name: categoryPath(categories, chosenCategory) })
    : category === "smart" ? t("importDialog.cat.smartHint") : t("importDialog.cat.noneHint");

  const start = async () => {
    const mode: Storage = storage === "index" ? "indexed" : "managed";
    setPhase("running");
    setStopping(false);
    try {
      const result = await importPaths(
        activeEntries.map((entry) => entry.path),
        mode,
        {
          categoryId: chosenCategory ?? undefined,
          autoClassify: smart,
          categoryPreviews: smart ? Object.fromEntries(activeEntries.map((entry) => [entry.path, previews.get(entry.path)!.category!])) : undefined,
          packages: [...packages],
          entries: Object.fromEntries(activeEntries.map((entry) => [entry.path, editOf(entry)])),
        },
        { move: storage === "move", quiet: true },
      );
      const done = result ?? { imported: 0, duplicates: 0, failed: 0, items: [], cancelled: true };
      setReport(done);
      // 导入时点了“在后台继续”，窗口已经关了：有失败的重新打开显示原因，都成功就提示一句
      const app = useApp.getState();
      if (app.importDialog !== state) {
        if ((done.failed > 0 || done.classificationError) && app.importDialog === null) app.patch({ importDialog: { paths: state.paths, report: done } });
        else app.notify(t("import.done", { imported: done.imported, duplicates: done.duplicates, failed: done.failed }), done.failed > 0 ? "error" : "info");
      }
    } catch (error) {
      setRunError(errorText(language, error));
      if (useApp.getState().importDialog !== state) useApp.getState().notify(errorText(language, error), "error");
    }
    setPhase("done");
  };

  // 只导入了一个软件时，“完成”顺手选中它。
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

  const title =
    phase === "confirm" ? t("importDialog.title", { n: entries ? activeEntries.length : state.paths.length }) : phase === "running" ? t("importDialog.running") : t("importDialog.doneTitle");

  const footer =
    phase === "confirm" ? (
      <>
        <Button variant="ghost" onClick={close}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" disabled={!library || activeEntries.length === 0 || activeEntries.some((entry) => !editOf(entry).name.trim()) || (smart && !previewsReady)} onClick={() => void start()}>
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
      <Button variant="primary" onClick={finish}>
        {t("importDialog.done")}
      </Button>
    );

  return (
    <Modal open onClose={close} title={title} width={smart && phase === "confirm" ? 800 : 720} footer={footer}>
      {phase === "confirm" && (
        <div className="flex flex-col gap-3">
          <EntryList
            entries={entries}
            error={inspectError}
            editOf={editOf}
            existingSoftware={existingSoftware}
            smart={smart}
            categoryOf={categoryOf}
            fixedName={Boolean(state.softwareName)}
            onEdit={updateDraft}
          />
          {smart && previewError && <p className="m-0 text-[12px] text-warning">
            {t("importDialog.categoryUnavailable")}: {previewError}{" "}
            <button type="button" className="text-accent underline" onClick={() => setPreviewAttempt((n) => n + 1)}>{t("setup.retry")}</button>
          </p>}
          <div className="grid grid-cols-2 gap-x-3 gap-y-1.5">
            <label className="flex min-w-0 flex-col gap-1 text-[12px] text-fg-2">
              {t("importDialog.category")}
              <Select value={category} onChange={(event) => setCategory(event.target.value)} title={categoryHint}>
                <option value="none">{t("importDialog.cat.none")}</option>
                <option value="smart">{t("importDialog.cat.smart")}</option>
                {current && <option value="current">{t("importDialog.cat.current")}</option>}
                {tree.length > 0 && <optgroup label={t("importDialog.cat.pick")}>
                  {tree.map(({ category, depth }) => (
                    <option key={category.id} value={`category:${category.id}`}>
                      {"　".repeat(depth) + category.name}
                    </option>
                  ))}
                </optgroup>}
              </Select>
            </label>
            <label className="flex min-w-0 flex-col gap-1 text-[12px] text-fg-2">
              {t("importDialog.storage")}
              <Select value={storage} onChange={(event) => setStorage(event.target.value as StorageMode)} title={storageHint[storage]}>
                <option value="copy">{storageLabel.copy}</option>
                <option value="move">{storageLabel.move}</option>
                <option value="index">{storageLabel.index}</option>
              </Select>
            </label>
            {storage === "move" && <p className="col-span-2 m-0 text-[12px] text-fg-3">{storageHint.move}</p>}
          </div>
        </div>
      )}

      {phase === "running" && <Running progress={progress} stopping={stopping} />}

      {phase === "done" && (runError ? <p className="m-0 text-[13px] text-danger">{runError}</p> : report && <Results report={report} />)}
    </Modal>
  );
}

/** 每个拖入的文件夹都作为一个软件导入。 */
function defaultPackages(entries: DropEntry[]): Set<string> {
  return new Set(entries.filter((e) => e.isDir).map((e) => e.path));
}

function EntryList({
  entries,
  error,
  editOf,
  existingSoftware,
  smart,
  categoryOf,
  fixedName,
  onEdit,
}: {
  entries: DropEntry[] | null;
  error: string | null;
  editOf: (entry: DropEntry) => ImportEntryEdit;
  existingSoftware: Map<string, Software>;
  smart: boolean;
  categoryOf: (entry: DropEntry, name: string) => string | undefined;
  fixedName: boolean;
  onEdit: (path: string, edit: Partial<ImportEntryEdit>) => void;
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
  const row = (entry: DropEntry, nested = false) => {
    const edit = editOf(entry);
    const matched = entry.matchedSoftwareId ? existingSoftware.get(entry.matchedSoftwareId) : undefined;
    return (
      <EntryRow
        key={entry.path}
        entry={entry}
        edit={edit}
        existingName={fixedName ? edit.name : edit.name === matched?.name ? matched?.name : undefined}
        icon={entry.iconData ?? (edit.name === matched?.name ? matched?.iconData : undefined)}
        category={categoryOf(entry, edit.name)}
        fixedName={fixedName}
        nested={nested}
        onEdit={(value) => onEdit(entry.path, value)}
      />
    );
  };
  return (
    <div className="max-h-[44vh] overflow-auto rounded-lg border border-line">
      <table className={cx("w-full table-fixed border-separate border-spacing-0 text-left text-[12px]", smart ? "min-w-[480px]" : "min-w-[420px]")}>
        <colgroup>
          <col className={smart ? "w-[24%]" : "w-[28%]"} />
          <col className={smart ? "w-[19%]" : "w-[23%]"} />
          <col className={smart ? "w-[14%]" : "w-[17%]"} />
          <col className={smart ? "w-[13%]" : "w-[16%]"} />
          <col className={smart ? "w-[13%]" : "w-[16%]"} />
          {smart && <col className="w-[17%]" />}
        </colgroup>
        <thead className="sticky top-0 z-10 bg-panel text-fg-2">
          <tr>
            {([
              "importDialog.sourceFile", "importDialog.softwareName", "importDialog.softwareVersion",
              "importDialog.metadataVersion", "importDialog.filenameVersion",
            ] as const).map((column) => (
              <th key={column} scope="col" className="border-b border-line px-2 py-2 font-medium leading-tight">{t(column)}</th>
            ))}
            {smart && <th scope="col" className="border-b border-line px-2 py-2 font-medium">{t("importDialog.category")}</th>}
          </tr>
        </thead>
        <tbody className="[&>tr:not(:last-child)>td]:border-b [&>tr>td]:border-line">
          {entries.map((entry) => row(entry))}
        </tbody>
      </table>
    </div>
  );
}

function ImportFile({ entry, icon }: { entry: DropEntry; icon?: string | null }) {
  const t = useT();
  const Icon = !entry.isDir ? FileBox : entry.name.toLowerCase().endsWith(".app") ? AppWindow : Folder;
  const details = [
    entry.path,
    entry.size !== null ? formatSize(entry.size) : "",
    !entry.isDir && !entry.installer ? t("importDialog.notInstaller") : "",
  ].filter(Boolean).join("\n");
  return (
    <span className="flex min-w-0 items-center gap-1.5">
      {icon ? <SoftwareIcon name={entry.guessName} icon={icon} size={18} /> : (
        <Icon size={18} strokeWidth={1.5} aria-hidden className="shrink-0 text-fg-3" />
      )}
      <MiddleTruncate text={entry.name} tail={11} title={details} className="min-w-0 text-fg" />
    </span>
  );
}

function EntryRow({
  entry, edit, existingName, icon, category, fixedName, nested, onEdit,
}: {
  entry: DropEntry;
  edit: ImportEntryEdit;
  existingName?: string;
  icon?: string | null;
  category?: string;
  fixedName: boolean;
  nested: boolean;
  onEdit: (edit: Partial<ImportEntryEdit>) => void;
}) {
  const t = useT();
  const inputClass = "h-7 w-full min-w-0 rounded-md border border-line bg-bg px-1.5 text-[12px] text-fg outline-none focus:border-accent";
  const sources = [
    { label: t("importDialog.metadataVersion"), value: entry.metadataVersion },
    { label: t("importDialog.filenameVersion"), value: entry.filenameVersion },
  ];
  const conflict = entry.metadataVersion && entry.filenameVersion && entry.metadataVersion !== entry.filenameVersion
    ? t("importDialog.versionConflict") : null;
  const joined = existingName ? t("importDialog.joinSoftware", { name: existingName }) : null;
  return (
    <tr>
      <td className={cx("px-2 py-1", nested && "pl-4")}>
        <ImportFile entry={entry} icon={icon} />
      </td>
      <td className="px-2 py-1">
        <div className="flex items-center gap-1">
          <input aria-label={t("importDialog.softwareName")} className={inputClass} value={edit.name} title={edit.name} readOnly={fixedName} onChange={(event) => onEdit({ name: event.target.value })} />
          {joined && <span title={joined} className="shrink-0 text-fg-3"><Link2 size={13} role="img" aria-label={joined} /></span>}
        </div>
      </td>
      <td className="px-2 py-1">
        <div className="flex items-center gap-1">
          <input aria-label={t("importDialog.softwareVersion")} className={cx(inputClass, "tabular-nums")} value={edit.version} title={edit.version} placeholder={t("importDialog.unknownVersion")} onChange={(event) => onEdit({ version: event.target.value })} />
          {conflict && <span title={conflict} className="shrink-0 text-warning"><TriangleAlert size={13} role="img" aria-label={conflict} /></span>}
        </div>
      </td>
      {sources.map(({ label, value }) => (
        <td key={label} className="px-2 py-1">
          {value ? (
            <button
              type="button"
              aria-label={`${label} ${value}`}
              aria-pressed={value === edit.version}
              title={value}
              onClick={() => onEdit({ version: value })}
              className="block max-w-full truncate rounded px-1.5 py-1 text-left tabular-nums text-fg-2 hover:bg-hover aria-pressed:bg-active aria-pressed:text-accent"
            >
              {value}
            </button>
          ) : <span title={t("importDialog.versionUnavailable")} className="px-1.5 text-fg-3">—</span>}
        </td>
      ))}
      {category !== undefined && <td className="px-2 py-1">
        <span className="block truncate text-fg-2" title={category}>{category}</span>
      </td>}
    </tr>
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
                : progress?.phase === "classify" ? t("importDialog.classifying") : t("importDialog.collecting")}
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
      {!!report.classified && <p className="m-0 text-[12px] text-fg-2">{t("importDialog.classified", { n: report.classified })}</p>}
      {report.classificationError && <p className="m-0 text-[12px] text-warning">
        {t("importDialog.classificationError", { error: errorText(language, report.classificationError) })}
      </p>}
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
