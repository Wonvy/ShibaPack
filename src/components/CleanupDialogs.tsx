// 清理用到的几个窗口：确认、结果、清理记录、清理设置（自定义文件夹和排除）。

import { CheckCircle2, FolderPlus, Plus, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, pickFolder } from "../lib/api";
import { pickedSize, runClean, skipReason, textOf, useCleanup } from "../lib/cleanup";
import { formatDate, formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { CleanAction, CleanItem, CleanOptions, CleanRun, CookieSite } from "../lib/types";
import { CleanIcon } from "../views/CleanupView";
import { Button, Checkbox, IconButton, Modal, TextInput, cx } from "./ui";

export function CleanupDialogs() {
  return (
    <>
      <ConfirmDialog />
      <ResultDialog />
      <HistoryDialog />
      <OptionsDialog />
    </>
  );
}

/** 确认：按怎么清分段（直接删除、移到回收站、清空回收站），清空回收站要再勾一次“我知道”。 */
function ConfirmDialog() {
  const t = useT();
  const language = useLanguage();
  const ids = useCleanup((state) => state.confirming);
  const all = useCleanup((state) => state.items);
  const skips = useCleanup((state) => state.skips);
  const keepCookies = useCleanup((state) => state.keepCookies);
  const [ack, setAck] = useState(false);
  useEffect(() => setAck(false), [ids]);
  const items = (all ?? []).filter((item) => ids?.includes(item.id));
  const close = () => useCleanup.setState({ confirming: null });
  const section = (action: CleanAction) => items.filter((item) => item.action === action);
  // 高影响的（清空回收站、隐私痕迹）删了就找不回来：要再勾一次“我知道”
  const needsAck = items.some((item) => item.risk === "high");
  const admin = items.filter((item) => item.admin).length;
  const withCookies = items.some((item) => item.cookies);
  const sections: { action: CleanAction; title: string; hint: string }[] = [
    {
      action: "delete",
      title: t("cleanup.confirm.delete", { size: formatSize(sum(section("delete"))) }),
      // 隐私痕迹也直接删，但不会自己长回来
      hint: t(section("delete").every((item) => item.risk === "safe") ? "cleanup.confirm.deleteHint" : "cleanup.confirm.deleteHintMixed"),
    },
    { action: "trash", title: t("cleanup.confirm.trash", { size: formatSize(sum(section("trash"))) }), hint: t("cleanup.confirm.trashHint") },
    { action: "emptyTrash", title: t("cleanup.confirm.empty"), hint: t("cleanup.confirm.emptyHint") },
    { action: "clearClipboard", title: t("cleanup.action.clearClipboard"), hint: t("cleanup.confirm.clipboardHint") },
  ];
  function sum(list: CleanItem[]) {
    return list.reduce((total, item) => total + pickedSize(item, skips), 0);
  }
  return (
    <Modal
      open={ids !== null && items.length > 0}
      onClose={close}
      title={t("cleanup.confirm.title", { n: items.length })}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button variant={needsAck ? "danger" : "primary"} disabled={needsAck && !ack} onClick={() => void runClean(items.map((item) => item.id))}>
            {t("cleanup.clean")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        {sections
          .filter((s) => section(s.action).length > 0)
          .map((s) => (
            <section key={s.action}>
              <div className="text-[13px] font-medium">{s.title}</div>
              <div className="mb-1.5 text-[12px] text-fg-3">{s.hint}</div>
              <div className="rounded-lg border border-line">
                {section(s.action).map((item) => {
                  const skipped = skips[item.id]?.length ?? 0;
                  return (
                    <div key={item.id} className="flex items-center gap-2.5 border-b border-line px-2.5 py-1.5 last:border-b-0">
                      <CleanIcon item={item} size={20} />
                      <span className="min-w-0 flex-1 truncate text-[13px]">{textOf(item.name, language)}</span>
                      {skipped > 0 && <span className="shrink-0 text-[12px] text-fg-3">{t("cleanup.confirm.skipped", { n: skipped })}</span>}
                      <span className="tabular shrink-0 text-[12px] text-fg-2">{item.size === null ? "—" : formatSize(pickedSize(item, skips))}</span>
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        {withCookies && (
          <p className="m-0 text-[12px] leading-relaxed text-fg-2">
            {keepCookies.length > 0 ? t("cleanup.confirm.keepCookies", { n: keepCookies.length }) : t("cleanup.confirm.noKeepCookies")}
          </p>
        )}
        {admin > 0 && <p className="m-0 text-[12px] leading-relaxed text-fg-2">{t("cleanup.confirm.admin", { n: admin })}</p>}
        {needsAck && (
          <Checkbox checked={ack} onChange={setAck}>
            {t("cleanup.confirm.ack")}
          </Checkbox>
        )}
      </div>
    </Modal>
  );
}

/** 结果：实际腾出多少、移到回收站多少、跳过了哪些和原因。 */
function ResultDialog() {
  const t = useT();
  const language = useLanguage();
  const report = useCleanup((state) => state.report);
  const items = useCleanup((state) => state.reportItems);
  const close = () => useCleanup.setState({ report: null });
  if (!report) return null;
  const name = (id: string) => {
    const item = items.find((x) => x.id === id);
    return item ? textOf(item.name, language) : id;
  };
  const nothing = report.removed === 0 && report.freed === 0 && report.trashed === 0;
  return (
    <Modal
      open
      onClose={close}
      title={t("cleanup.result.title")}
      width={440}
      footer={
        <Button variant="primary" onClick={close}>
          {t("common.close")}
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        {nothing ? (
          <p className="m-0 text-[13px] text-fg-2">{t("cleanup.result.none")}</p>
        ) : (
          // 只列有的：直接删除的、移到回收站的
          <div className={cx("grid gap-2", report.freed > 0 && report.trashed > 0 ? "grid-cols-2" : "grid-cols-1")}>
            {(report.freed > 0 || report.trashed === 0) && <Figure label={t("cleanup.result.freed")} value={formatSize(report.freed)} strong />}
            {report.trashed > 0 && <Figure label={t("cleanup.result.trashed")} value={formatSize(report.trashed)} hint={t("cleanup.result.trashedHint")} />}
          </div>
        )}
        <div className="flex items-center gap-2 text-[12px] text-fg-2">
          <CheckCircle2 size={14} strokeWidth={1.75} className="text-success" aria-hidden />
          {t("cleanup.result.removed", { n: report.removed })}
          {report.skippedCount > 0 && <span className="text-warning">· {t("cleanup.result.skipped", { n: report.skippedCount })}</span>}
        </div>
        {report.skipped.length > 0 && (
          <div className="max-h-[240px] overflow-y-auto rounded-lg border border-line">
            {report.skipped.map((skip, i) => (
              <div key={i} className="border-b border-line px-2.5 py-1.5 last:border-b-0">
                <div className="flex items-center gap-2 text-[12px]">
                  <span className="min-w-0 flex-1 truncate font-medium">{name(skip.item)}</span>
                  <span className="shrink-0 text-warning">{skipReason(skip.reason)}</span>
                </div>
                {skip.path && (
                  <div className="selectable truncate font-mono text-[12px] text-fg-3" title={skip.path}>
                    {skip.path}
                  </div>
                )}
              </div>
            ))}
            {report.skippedCount > report.skipped.length && (
              <div className="px-2.5 py-1.5 text-[12px] text-fg-3">{t("cleanup.result.skippedMore", { n: report.skippedCount - report.skipped.length })}</div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}

function Figure({ label, value, hint, strong }: { label: string; value: string; hint?: string; strong?: boolean }) {
  return (
    <div className="rounded-lg border border-line px-3 py-2.5">
      <div className="text-[12px] text-fg-3">{label}</div>
      <div className={cx("tabular text-[18px] font-semibold", strong ? "text-fg" : "text-fg-2")}>{value}</div>
      {hint && <div className="mt-0.5 text-[12px] leading-snug text-fg-3">{hint}</div>}
    </div>
  );
}

/** 清理记录：最近 20 次。 */
function HistoryDialog() {
  const t = useT();
  const language = useLanguage();
  const open = useCleanup((state) => state.historyOpen);
  const [runs, setRuns] = useState<CleanRun[] | null>(null);
  useEffect(() => {
    if (!open) return;
    setRuns(null);
    void api.cleanupHistory().then(setRuns, () => setRuns([]));
  }, [open]);
  const close = () => useCleanup.setState({ historyOpen: false });
  return (
    <Modal open={open} onClose={close} title={t("cleanup.history.title")} width={460}>
      <p className="mt-0 mb-3 text-[12px] text-fg-3">{t("cleanup.history.note")}</p>
      {runs !== null && runs.length === 0 && <p className="m-0 py-6 text-center text-[13px] text-fg-3">{t("cleanup.history.empty")}</p>}
      <div className="flex flex-col gap-2">
        {(runs ?? []).map((run) => (
          <section key={run.id} className="rounded-lg border border-line px-3 py-2.5">
            <div className="flex items-baseline gap-2">
              <span className="text-[13px] font-medium">{formatDate(run.at, language, true)}</span>
              <span className="ml-auto truncate text-[12px] text-fg-3">{t("cleanup.history.summary", { freed: formatSize(run.freed), trashed: formatSize(run.trashed) })}</span>
            </div>
            {run.items.length > 0 && (
              <ul className="m-0 mt-1.5 list-none p-0">
                {run.items.map((item) => (
                  <li key={item.id} className="flex items-center gap-2 py-0.5 text-[12px] text-fg-2">
                    <span className="min-w-0 flex-1 truncate">{textOf(item.name, language)}</span>
                    <span className="tabular shrink-0 text-fg-3">{formatSize(item.freed + item.trashed)}</span>
                  </li>
                ))}
              </ul>
            )}
            {run.skipped > 0 && <div className="mt-1 text-[12px] text-warning">{t("cleanup.history.skipped", { n: run.skipped })}</div>}
          </section>
        ))}
      </div>
    </Modal>
  );
}

/** 清理设置：自定义清理文件夹（过保护检查才能加）和排除的文件夹。改完马上存，并重新扫描。 */
function OptionsDialog() {
  const t = useT();
  const open = useCleanup((state) => state.optionsOpen);
  const attempt = useApp((state) => state.attempt);
  const [options, setOptions] = useState<CleanOptions | null>(null);
  const [changed, setChanged] = useState(false);
  // 改动一个一个存，每次都在上一次存好的结果上改：连着点两下不会把前一下盖掉
  const latest = useRef<CleanOptions | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  useEffect(() => {
    if (!open) return;
    setChanged(false);
    const loaded = (value: CleanOptions) => {
      latest.current = value;
      setOptions(value);
    };
    void api.cleanupOptions().then(loaded, () => loaded({ custom: [], exclude: [], keepCookies: [] }));
  }, [open]);
  const update = (change: (current: CleanOptions) => CleanOptions | null): Promise<boolean> => {
    const step = queue.current.then(async () => {
      const current = latest.current;
      const next = current && change(current);
      if (!next) return false;
      const saved = await attempt(() => api.cleanupSetOptions(next));
      if (!saved) return false;
      latest.current = saved;
      setOptions(saved);
      setChanged(true);
      useCleanup.setState({ keepCookies: saved.keepCookies });
      return true;
    });
    queue.current = step;
    return step;
  };
  const close = () => {
    useCleanup.setState({ optionsOpen: false });
    if (changed) void useCleanup.getState().scan();
  };
  const add = async (key: "custom" | "exclude") => {
    const folder = await attempt(pickFolder);
    if (folder) await update((current) => (current[key].includes(folder) ? null : { ...current, [key]: [...current[key], folder] }));
  };
  const remove = (key: keyof CleanOptions, value: string) => {
    void update((current) => ({ ...current, [key]: current[key].filter((x) => x !== value) }));
  };
  return (
    <Modal
      open={open}
      onClose={close}
      title={t("cleanup.options.title")}
      width={480}
      footer={
        <Button variant="primary" onClick={close}>
          {t("common.close")}
        </Button>
      }
    >
      {options && (
        <div className="flex flex-col gap-5">
          <FolderList
            title={t("cleanup.options.custom")}
            hint={t("cleanup.options.customHint")}
            folders={options.custom}
            onAdd={() => void add("custom")}
            onRemove={(folder) => remove("custom", folder)}
          />
          <FolderList
            title={t("cleanup.options.exclude")}
            hint={t("cleanup.options.excludeHint")}
            folders={options.exclude}
            onAdd={() => void add("exclude")}
            onRemove={(folder) => remove("exclude", folder)}
          />
          <KeepCookies
            sites={options.keepCookies}
            onAdd={(site) => update((current) => ({ ...current, keepCookies: [...current.keepCookies, site] }))}
            onRemove={(site) => remove("keepCookies", site)}
          />
        </div>
      )}
    </Modal>
  );
}

function FolderList({ title, hint, folders, onAdd, onRemove }: { title: string; hint: string; folders: string[]; onAdd: () => void; onRemove: (folder: string) => void }) {
  const t = useT();
  return (
    <section>
      <div className="flex items-center gap-2">
        <h3 className="m-0 flex-1 text-[13px] font-medium">{title}</h3>
        <Button size="sm" onClick={onAdd}>
          <FolderPlus size={13} strokeWidth={1.75} aria-hidden />
          {t("cleanup.options.add")}
        </Button>
      </div>
      <p className="mt-1 mb-2 text-[12px] leading-relaxed text-fg-3">{hint}</p>
      <div className="rounded-lg border border-line">
        {folders.length === 0 && <div className="px-3 py-2 text-[12px] text-fg-3">{t("cleanup.options.none")}</div>}
        {folders.map((folder) => (
          <div key={folder} className="flex items-center gap-2 border-b border-line py-1 pr-1 pl-3 last:border-b-0">
            <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px]" title={folder}>
              {folder}
            </span>
            <IconButton label={t("cleanup.options.remove")} className="size-6" onClick={() => onRemove(folder)}>
              <X size={13} strokeWidth={1.75} />
            </IconButton>
          </div>
        ))}
      </div>
    </section>
  );
}

/** Cookie 保留名单：手动输入网址或域名，或者从浏览器里有 Cookie 的网站里点选。 */
function KeepCookies({ sites, onAdd, onRemove }: { sites: string[]; onAdd: (site: string) => Promise<boolean>; onRemove: (site: string) => void }) {
  const t = useT();
  const [draft, setDraft] = useState("");
  const [suggestions, setSuggestions] = useState<CookieSite[] | null>(null);
  useEffect(() => {
    void api.cleanupCookieSites().then(setSuggestions, () => setSuggestions([]));
  }, []);
  const submit = async () => {
    const value = draft.trim();
    if (value && (await onAdd(value))) setDraft("");
  };
  const offered = (suggestions ?? []).filter((s) => !sites.includes(s.site)).slice(0, 16);
  return (
    <section>
      <h3 className="m-0 text-[13px] font-medium">{t("cleanup.options.keepCookies")}</h3>
      <p className="mt-1 mb-2 text-[12px] leading-relaxed text-fg-3">{t("cleanup.options.keepCookiesHint")}</p>
      <div className="flex gap-2">
        <TextInput
          className="flex-1"
          value={draft}
          placeholder={t("cleanup.options.domainPlaceholder")}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void submit();
          }}
        />
        <Button disabled={!draft.trim()} onClick={() => void submit()}>
          {t("cleanup.options.addDomain")}
        </Button>
      </div>
      {sites.length > 0 && (
        <div className="mt-2 flex flex-wrap gap-1.5">
          {sites.map((site) => (
            <span key={site} className="inline-flex h-6 items-center gap-1 rounded-md border border-line-strong pr-0.5 pl-2 text-[12px]">
              {site}
              <button type="button" aria-label={t("cleanup.options.remove")} title={t("cleanup.options.remove")} className="flex size-5 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg" onClick={() => onRemove(site)}>
                <X size={12} strokeWidth={2} />
              </button>
            </span>
          ))}
        </div>
      )}
      {offered.length > 0 && (
        <>
          <div className="mt-3 mb-1.5 text-[12px] text-fg-3">{t("cleanup.options.suggestions")}</div>
          <div className="flex flex-wrap gap-1.5">
            {offered.map((s) => (
              <button
                key={s.site}
                type="button"
                className="inline-flex h-6 items-center gap-1.5 rounded-md bg-hover px-2 text-[12px] text-fg-2 hover:bg-active hover:text-fg"
                onClick={() => void onAdd(s.site)}
              >
                <Plus size={12} strokeWidth={2} aria-hidden />
                {s.site}
                <span className="tabular text-fg-3">{s.count}</span>
              </button>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
