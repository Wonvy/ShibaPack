// 路径：收藏软件的配置文件夹、配置文件这类放在别处的路径，写上备注，可以备份进仓库、从备份恢复。
// 从系统把文件或文件夹拖进这一页就收藏下来。右边是详情栏：备注、路径、备份列表。

import {
  AlertTriangle,
  Archive,
  CheckSquare,
  ChevronDown,
  Copy,
  File,
  Folder,
  FolderHeart,
  FolderOpen,
  FolderSearch,
  LoaderCircle,
  MousePointerClick,
  MoreHorizontal,
  PenLine,
  Plus,
  RefreshCw,
  RotateCcw,
  SearchX,
  Trash2,
  TextCursorInput,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { prompt } from "../components/dialogs";
import { Button, Chip, ContextMenu, Dropdown, EmptyState, IconButton, type MenuEntry, Switch, cx } from "../components/ui";
import { api, pickPaths } from "../lib/api";
import { formatDate, formatSize } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { addPaths, backupPaths, deleteBackup, pathTitle, removePaths, restoreBackup, usePaths } from "../lib/paths";
import { useApp, useLanguage, useT } from "../lib/store";
import type { PathState, SavedPath } from "../lib/types";

const EMPTY: SavedPath[] = [];

export function PathsView({ detailToggle }: { detailToggle: ReactNode }) {
  const t = useT();
  const items = useApp((state) => state.library?.paths ?? EMPTY);
  const search = useApp((state) => state.search);
  const route = useApp((state) => state.route);
  const states = usePaths((state) => state.states);
  const loading = usePaths((state) => state.loading);
  const selectedId = usePaths((state) => state.selectedId);
  const selectedIds = usePaths((state) => state.selectedIds);
  const select = usePaths((state) => state.select);
  const setSelection = usePaths((state) => state.setSelection);
  const load = usePaths((state) => state.load);
  const busyAny = usePaths((state) => Object.keys(state.busy).length > 0);

  // 进来时看一遍：在不在、有几份备份。收藏的路径变了（别处加的、删的）也重新看
  const ids = items.map((item) => item.id).join(",");
  useEffect(() => {
    void load();
  }, [load, ids]);

  const list = useMemo(() => {
    const query = search.trim().toLowerCase();
    const shown = items.filter((item) => !query || [item.note, item.path, states?.[item.id]?.full ?? ""].some((text) => text.toLowerCase().includes(query)));
    return [...shown].sort((a, b) => pathTitle(a).localeCompare(pathTitle(b), undefined, { numeric: true, sensitivity: "base" }));
  }, [items, search, states]);
  const chosen = selectedIds.length > 0 ? selectedIds : selectedId ? [selectedId] : [];

  const clickRow = (id: string, event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => {
    if (event.metaKey || event.ctrlKey) {
      const next = chosen.includes(id) ? chosen.filter((x) => x !== id) : [...chosen, id];
      setSelection(next, next.includes(id) ? id : (next[0] ?? null));
    } else if (event.shiftKey && selectedId && list.some((item) => item.id === selectedId)) {
      const from = list.findIndex((item) => item.id === selectedId);
      const to = list.findIndex((item) => item.id === id);
      setSelection(list.slice(Math.min(from, to), Math.max(from, to) + 1).map((item) => item.id), selectedId);
    } else {
      select(id);
    }
  };

  // ⌘ / Ctrl + A 全选，Esc 取消多选，Delete / Backspace 取消收藏
  useEffect(() => {
    if (route !== "paths") return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=true]")) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setSelection(list.map((item) => item.id), selectedId ?? list[0]?.id ?? null);
      } else if (event.key === "Escape" && chosen.length > 1) {
        select(selectedId);
      } else if ((event.key === "Delete" || event.key === "Backspace") && chosen.length > 0) {
        event.preventDefault();
        void removePaths(chosen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [route, list, selectedId, chosen, setSelection, select]);

  const filtering = search.trim() !== "";

  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-line pr-2 pl-4">
        <span className="text-[13px] font-semibold">{t("nav.paths")}</span>
        {items.length > 0 && <span className="tabular text-[12px] text-fg-3">{items.length}</span>}
        <div className="ml-auto flex items-center gap-0.5">
          <AddPathMenu>
            <IconButton label={t("paths.add")}>
              <Plus size={15} strokeWidth={1.75} />
            </IconButton>
          </AddPathMenu>
          <IconButton
            label={t("paths.backupAll")}
            disabled={items.length === 0 || busyAny}
            onClick={() => void backupPaths(list.filter((item) => states?.[item.id]?.exists).map((item) => item.id))}
          >
            <Archive size={15} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("paths.refresh")} disabled={loading} onClick={() => void load()}>
            <RefreshCw size={15} strokeWidth={1.75} className={loading ? "animate-spin" : ""} />
          </IconButton>
          {detailToggle}
        </div>
      </div>

      {items.length === 0 ? (
        <EmptyState
          icon={FolderHeart}
          title={t("paths.empty")}
          body={t("paths.emptyBody")}
          action={
            <AddPathMenu align="start">
              <Button size="sm">
                <Plus size={14} strokeWidth={1.75} aria-hidden />
                {t("paths.add")}
              </Button>
            </AddPathMenu>
          }
        />
      ) : list.length === 0 && filtering ? (
        <EmptyState icon={SearchX} title={t("lib.noMatch")} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-10" onClick={(event) => event.target === event.currentTarget && select(null)}>
          {list.map((item) => (
            <PathRow
              key={item.id}
              item={item}
              state={states?.[item.id]}
              selected={chosen.includes(item.id)}
              group={chosen.length > 1 && chosen.includes(item.id) ? chosen : null}
              onSelect={(event) => clickRow(item.id, event)}
            />
          ))}
          <p className="m-0 px-3 pt-3 text-center text-[12px] text-fg-4">{t("paths.dropHint")}</p>
        </div>
      )}
    </div>
  );
}

/** 添加：选文件夹、选文件，或者手动输入（可以带环境变量）。 */
function AddPathMenu({ children, align = "end" }: { children: ReactNode; align?: "start" | "end" }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const convert = usePrefs((state) => state.pathsConvert);
  const pick = async (directory: boolean) => {
    const picked = await attempt(() => pickPaths(directory));
    if (picked?.length) await addPaths(picked);
  };
  const type = async () => {
    const text = await prompt({ title: t("paths.typeTitle"), label: t("paths.typeLabel"), placeholder: t("paths.typePlaceholder") });
    if (text?.trim()) await addPaths([text.trim()]);
  };
  return (
    <Dropdown
      align={align}
      items={[
        { label: t("paths.addFolder"), icon: Folder, onSelect: () => void pick(true) },
        { label: t("paths.addFile"), icon: File, onSelect: () => void pick(false) },
        { label: t("paths.addTyped"), icon: TextCursorInput, onSelect: () => void type() },
        { label: t("paths.autoConvert"), checked: convert, separated: true, onSelect: () => usePrefs.getState().set({ pathsConvert: !convert }) },
      ]}
    >
      {children}
    </Dropdown>
  );
}

function usePathMenu(item: SavedPath, state: PathState | undefined, group: string[] | null): MenuEntry[] {
  const t = useT();
  const attempt = useApp((s) => s.attempt);
  const notify = useApp((s) => s.notify);
  if (group) {
    return [
      { label: t("paths.backupMany", { n: group.length }), icon: Archive, onSelect: () => void backupPaths(group) },
      { label: t("paths.removeMany", { n: group.length }), icon: Trash2, danger: true, separated: true, onSelect: () => void removePaths(group) },
    ];
  }
  const exists = state?.exists ?? false;
  return [
    { label: t("paths.backupNow"), icon: Archive, disabled: !exists, onSelect: () => void backupPaths([item.id]) },
    { label: t("paths.reveal"), icon: FolderSearch, disabled: !exists, separated: true, onSelect: () => void attempt(() => api.pathReveal(item.id, false)) },
    ...(state?.isDir ? [{ label: t("paths.open"), icon: FolderOpen, disabled: !exists, onSelect: () => void attempt(() => api.pathReveal(item.id, true)) }] : []),
    {
      label: t("paths.copy"),
      icon: Copy,
      onSelect: () => void attempt(() => navigator.clipboard.writeText(state?.full || item.path)).then(() => notify(t("paths.copied"))),
    },
    { label: t("paths.editNote"), icon: PenLine, separated: true, onSelect: () => void editNote(item, t) },
    { label: t("paths.editPath"), icon: TextCursorInput, onSelect: () => void editPath(item, t) },
    { label: t("paths.remove"), icon: Trash2, danger: true, separated: true, onSelect: () => void removePaths([item.id]) },
  ];
}

type T = ReturnType<typeof useT>;

async function editNote(item: SavedPath, t: T) {
  const note = await prompt({ title: t("paths.editNote"), label: t("paths.note"), initial: item.note, placeholder: t("paths.notePlaceholder"), multiline: true });
  if (note === null) return;
  await useApp.getState().apply(() => api.pathEdit(item.id, { note }));
}

async function editPath(item: SavedPath, t: T) {
  const path = await prompt({ title: t("paths.editPath"), label: t("paths.typeLabel"), initial: item.path, placeholder: t("paths.typePlaceholder") });
  if (path === null || !path.trim() || path.trim() === item.path) return;
  const ok = await useApp.getState().apply(() => api.pathEdit(item.id, { path: path.trim(), convert: usePrefs.getState().pathsConvert }));
  if (ok) await usePaths.getState().load();
}

/**
 * 切换路径的写法：用变量（换电脑、换用户名也对得上）还是完整路径。Windows 上能用好几个变量时都列出来。
 * 选了就照写，不再自动换。
 */
function FormSwitch({ item, forms }: { item: SavedPath; forms: string[] }) {
  const t = useT();
  const full = forms[forms.length - 1];
  const current = forms.find((form) => form === item.path) ?? (item.path === full ? full : null);
  const usingVar = current !== full;
  const choose = async (form: string) => {
    if (form === item.path) return;
    const ok = await useApp.getState().apply(() => api.pathEdit(item.id, { path: form, convert: false }));
    if (ok) await usePaths.getState().load();
  };
  // 只有两种写法（macOS 上的 ~ 和完整路径）：一个开关；Windows 上有好几个变量可选：一个菜单
  if (forms.length === 2) {
    return (
      <label className="ml-auto flex cursor-pointer items-center gap-1.5 text-[12px] text-fg-3">
        {t("paths.useVariable")}
        <Switch checked={usingVar} label={t("paths.useVariable")} onChange={(on) => void choose(on ? forms[0] : full)} />
      </label>
    );
  }
  return (
    <span className="ml-auto">
      <Dropdown
        items={forms.map((form, index) => ({
          label: index === forms.length - 1 ? `${t("paths.fullPath")}  ${form}` : form,
          checked: form === current,
          onSelect: () => void choose(form),
        }))}
      >
        <button type="button" className="flex items-center gap-0.5 rounded px-1 text-[12px] text-fg-3 hover:bg-hover hover:text-fg">
          {usingVar ? t("paths.formVariable") : t("paths.fullPath")}
          <ChevronDown size={13} strokeWidth={1.75} aria-hidden />
        </button>
      </Dropdown>
    </span>
  );
}

/** 路径的图标：文件夹、文件；不在了或者展开不了时是警告。 */
function KindIcon({ item, state, size }: { item: SavedPath; state: PathState | undefined; size: number }) {
  const missing = state && !state.exists;
  const Icon = missing ? AlertTriangle : (state?.isDir ?? item.isDir) ? Folder : File;
  return (
    <span
      className={cx(
        "flex shrink-0 items-center justify-center rounded-md",
        missing ? "bg-[color-mix(in_srgb,var(--warning)_13%,transparent)] text-warning" : "bg-accent-soft text-accent",
      )}
      style={{ width: size, height: size }}
    >
      <Icon size={Math.round(size * 0.55)} strokeWidth={1.75} aria-hidden />
    </span>
  );
}

function PathRow({
  item,
  state,
  selected,
  group,
  onSelect,
}: {
  item: SavedPath;
  state: PathState | undefined;
  selected: boolean;
  group: string[] | null;
  onSelect: (event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => void;
}) {
  const t = useT();
  const language = useLanguage();
  const menu = usePathMenu(item, state, group);
  const select = usePaths((s) => s.select);
  const busy = usePaths((s) => s.busy[item.id]);
  const title = pathTitle(item);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        onClick={onSelect}
        onContextMenu={() => !group && !selected && select(item.id)}
        onKeyDown={(event) => event.target === event.currentTarget && event.key === "Enter" && onSelect(event)}
        className={cx("group flex min-h-[50px] items-center gap-2.5 rounded-md py-1.5 pr-3 pl-2.5 transition-colors", selected ? "bg-active" : "hover:bg-hover")}
      >
        <KindIcon item={item} state={state} size={28} />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <span className="truncate text-[13px] font-medium">{title}</span>
            {state && !state.exists && <Chip className="h-[18px] px-1 text-warning">{state.error ? t("paths.badVar") : t("paths.missing")}</Chip>}
            {busy && <LoaderCircle size={12} strokeWidth={2} className="shrink-0 animate-spin text-accent" aria-label={t(busy === "backup" ? "paths.backingUp" : "paths.restoring")} />}
          </div>
          <div className="truncate text-[12px] text-fg-3" title={state?.full || item.path}>
            {item.path}
          </div>
        </div>
        <div className="hidden shrink-0 text-right text-[12px] text-fg-3 @[420px]:block">
          {state?.lastBackup ? (
            <>
              <div className="tabular">{formatDate(state.lastBackup, language, true)}</div>
              <div className="tabular text-fg-4">{t("paths.backupCount", { n: state.backups, size: formatSize(state.backupSize) })}</div>
            </>
          ) : (
            <span className="text-fg-4">{t("paths.neverBackedUp")}</span>
          )}
        </div>
      </div>
    </ContextMenu>
  );
}

// ── 详情栏 ───────────────────────────────────────────────────

export function PathsDetail() {
  const many = usePaths((state) => state.selectedIds.length > 1);
  return many ? <PathsBatch /> : <PathSingle />;
}

function PathsBatch() {
  const t = useT();
  const ids = usePaths((state) => state.selectedIds);
  const select = usePaths((state) => state.select);
  const busy = usePaths((state) => ids.some((id) => state.busy[id]));
  return (
    <div className="flex h-full flex-col px-4 pt-4">
      <div className="flex items-center gap-2">
        <CheckSquare size={18} strokeWidth={1.75} aria-hidden className="text-accent" />
        <span className="text-[15px] font-semibold">{t("paths.selected", { n: ids.length })}</span>
      </div>
      <div className="mt-3 flex flex-col gap-2">
        <Button variant="primary" busy={busy} onClick={() => void backupPaths(ids)}>
          {!busy && <Archive size={14} strokeWidth={1.75} aria-hidden />}
          {t("paths.backupMany", { n: ids.length })}
        </Button>
        <Button onClick={() => void removePaths(ids)}>
          <Trash2 size={14} strokeWidth={1.75} aria-hidden />
          {t("paths.removeMany", { n: ids.length })}
        </Button>
        <Button variant="ghost" onClick={() => select(null)}>
          {t("batch.clear")}
        </Button>
      </div>
    </div>
  );
}

function PathSingle() {
  const t = useT();
  const language = useLanguage();
  const selectedId = usePaths((state) => state.selectedId);
  const item = useApp((state) => state.library?.paths?.find((p) => p.id === selectedId));
  const state = usePaths((s) => (selectedId ? s.states?.[selectedId] : undefined));
  const backups = usePaths((s) => (selectedId ? s.backups[selectedId] : undefined));
  const busy = usePaths((s) => (selectedId ? s.busy[selectedId] : undefined));
  const loadBackups = usePaths((s) => s.loadBackups);
  const attempt = useApp((s) => s.attempt);
  const notify = useApp((s) => s.notify);
  const [note, setNote] = useState(item?.note ?? "");

  useEffect(() => {
    if (selectedId) void loadBackups(selectedId);
  }, [selectedId, loadBackups]);
  useEffect(() => setNote(item?.note ?? ""), [item?.id, item?.note]);

  if (!item) return <EmptyState icon={MousePointerClick} title={t("paths.select")} />;

  const saveNote = () => {
    if (note.trim() !== item.note) void useApp.getState().apply(() => api.pathEdit(item.id, { note }));
  };
  const exists = state?.exists ?? false;
  const portable = state?.full && state.full !== item.path;

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-3">
          <KindIcon item={item} state={state} size={44} />
          <div className="min-w-0">
            <div className="selectable truncate text-[15px] font-semibold" title={pathTitle(item)}>
              {pathTitle(item)}
            </div>
            <div className="truncate text-[12px] text-fg-2">{t((state?.isDir ?? item.isDir) ? "paths.kindFolder" : "paths.kindFile")}</div>
          </div>
        </div>

        {state && !state.exists && (
          <div className="mt-3 flex gap-2 rounded-lg bg-[color-mix(in_srgb,var(--warning)_13%,transparent)] px-3 py-2 text-[12px] leading-relaxed text-warning">
            <AlertTriangle size={15} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
            <span>{state.error ? t("paths.badVarHint", { name: state.error.split(":")[1]?.trim() ?? "" }) : t("paths.missingHint")}</span>
          </div>
        )}

        <div className="mt-4 flex gap-2">
          <Button variant="primary" className="flex-1" busy={busy === "backup"} disabled={!exists || Boolean(busy)} onClick={() => void backupPaths([item.id])}>
            {busy !== "backup" && <Archive size={14} strokeWidth={1.75} aria-hidden />}
            {t("paths.backupNow")}
          </Button>
          <IconButton label={t("paths.reveal")} disabled={!exists} onClick={() => void attempt(() => api.pathReveal(item.id, false))}>
            <FolderSearch size={15} strokeWidth={1.75} />
          </IconButton>
          <IconButton
            label={t("paths.copy")}
            onClick={() => void attempt(() => navigator.clipboard.writeText(state?.full || item.path)).then(() => notify(t("paths.copied")))}
          >
            <Copy size={15} strokeWidth={1.75} />
          </IconButton>
          <Dropdown
            items={[
              { label: t("paths.editPath"), icon: TextCursorInput, onSelect: () => void editPath(item, t) },
              { label: t("paths.remove"), icon: Trash2, danger: true, separated: true, onSelect: () => void removePaths([item.id]) },
            ]}
          >
            <IconButton label={t("paths.more")}>
              <MoreHorizontal size={15} strokeWidth={1.75} />
            </IconButton>
          </Dropdown>
        </div>

        <label className="mt-4 block text-[12px] font-medium text-fg-2" htmlFor="path-note">
          {t("paths.note")}
        </label>
        <textarea
          id="path-note"
          value={note}
          rows={2}
          placeholder={t("paths.notePlaceholder")}
          onChange={(event) => setNote(event.target.value)}
          onBlur={saveNote}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) (event.target as HTMLTextAreaElement).blur();
          }}
          className="mt-1 w-full resize-y rounded-md border border-line-strong bg-bg px-2.5 py-1.5 text-[13px] leading-relaxed text-fg outline-none transition-colors placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent"
        />

        <div className="mt-3 flex items-center gap-2">
          <span className="text-[12px] font-medium text-fg-2">{t("paths.path")}</span>
          {state && state.forms.length > 1 && <FormSwitch item={item} forms={state.forms} />}
        </div>
        <div className="selectable mt-1 rounded-md bg-hover px-2.5 py-1.5 font-mono text-[12px] leading-relaxed break-all">{item.path}</div>
        {portable && (
          <div className="selectable mt-1 px-0.5 text-[12px] leading-relaxed break-all text-fg-3">
            {t("paths.expanded")} {state?.full}
          </div>
        )}

        <div className="mt-5 flex items-center gap-2">
          <span className="text-[12px] font-medium text-fg-2">{t("paths.backups")}</span>
          {backups && backups.length > 0 && <span className="tabular text-[12px] text-fg-3">{backups.length}</span>}
        </div>
        {backups === undefined ? (
          <div className="mt-2 text-[12px] text-fg-4">{t("paths.loading")}</div>
        ) : backups.length === 0 ? (
          <p className="m-0 mt-1.5 text-[12px] leading-relaxed text-fg-3">{t("paths.noBackups")}</p>
        ) : (
          <div className="mt-1.5 overflow-hidden rounded-lg border border-line">
            {backups.map((backup) => (
              <div key={backup.name} className="group flex items-center gap-2 border-b border-line px-3 py-2 last:border-b-0">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="tabular text-[13px]">{formatDate(backup.createdAt, language, true)}</span>
                    {backup.beforeRestore && <Chip className="h-[18px] px-1">{t("paths.beforeRestore")}</Chip>}
                  </div>
                  <div className="tabular text-[12px] text-fg-3">
                    {t("paths.backupFacts", { size: formatSize(backup.size), n: backup.files })}
                    {backup.skippedCount > 0 && <span className="text-warning"> · {t("paths.skipped", { n: backup.skippedCount })}</span>}
                  </div>
                </div>
                <IconButton label={t("paths.restore")} disabled={Boolean(busy)} onClick={() => void restoreBackup(item.id, backup)}>
                  <RotateCcw size={15} strokeWidth={1.75} />
                </IconButton>
                <Dropdown
                  items={[
                    { label: t("paths.revealBackup"), icon: FolderSearch, onSelect: () => void attempt(() => api.pathRevealBackup(item.id, backup.name)) },
                    { label: t("paths.deleteBackup"), icon: Trash2, danger: true, separated: true, onSelect: () => void deleteBackup(item.id, backup) },
                  ]}
                >
                  <IconButton label={t("paths.more")}>
                    <MoreHorizontal size={15} strokeWidth={1.75} />
                  </IconButton>
                </Dropdown>
              </div>
            ))}
          </div>
        )}
        <p className="m-0 mt-2 text-[12px] leading-relaxed text-fg-4">{t("paths.backupWhere")}</p>
      </div>
    </div>
  );
}
