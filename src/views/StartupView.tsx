// 启动与服务：登录或开机时自动运行的程序和后台服务，可以启用、停用、启动、停止。右边是详情栏。

import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  CheckSquare,
  ChevronDown,
  Circle,
  CircleDot,
  Cog,
  Copy,
  ExternalLink,
  FolderOpen,
  ListFilter,
  LoaderCircle,
  MousePointerClick,
  Play,
  Power,
  PowerOff,
  RefreshCw,
  RotateCw,
  Rocket,
  SearchX,
  ShieldCheck,
  Sparkles,
  Square,
  Users,
} from "lucide-react";
import { type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import { DetailToggle } from "../components/chrome";
import { type ColumnDef, type Columns, ColumnsContext, TableHeader, useTableColumns, useWidth } from "../components/columns";
import { StartupIcon as ItemIcon } from "../components/StartupIcon";
import { Button, Chip, ContextMenu, Dropdown, EmptyState, FilterMenu, IconButton, type MenuEntry, Segmented, Switch, cx } from "../components/ui";
import { aiUsable } from "../lib/ai";
import { api, hostPlatform } from "../lib/api";
import type { Key } from "../lib/i18n";
import { usePrefs } from "../lib/prefs";
import { explainMany, explainStartup, loadExplanation, startTypeLabel, startupAct, subtitle, triggerText, useStartup } from "../lib/startup";
import { formatDate } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { StartupAction, StartupItem } from "../lib/types";

type StatusFilter = "all" | "enabled" | "disabled" | "running";
type SortColumn = "name" | "source" | "status" | "auto";

// 列表的列：名称一直在，右边几列的宽度在表头上拖，右键表头勾选显示哪几列；栏窄时来源、状态先收起。
// 启动项和服务各记各的列宽。“自动运行”一列在 Windows 服务里放启动类型的菜单，默认宽一些
type StartupColumn = "source" | "status" | "auto";
const STARTUP_COLUMNS: ColumnDef<StartupColumn>[] = [
  { key: "source", initial: 100, min: 72, max: 220, autoHide: 1 },
  { key: "status", initial: 84, min: 64, max: 180, autoHide: 2 },
  { key: "auto", initial: 72, min: 64, max: 180, end: true },
];
const SERVICE_COLUMNS: ColumnDef<StartupColumn>[] = [
  { key: "status", initial: 84, min: 64, max: 180, autoHide: 2 },
  { key: "auto", initial: 72, min: 64, max: 180, end: true },
];
const SERVICE_COLUMNS_WIDE: ColumnDef<StartupColumn>[] = [
  { key: "status", initial: 84, min: 64, max: 180, autoHide: 2 },
  { key: "auto", initial: 124, min: 64, max: 200, end: true },
];
/** 一行里除了各列之外占的宽度：列表的左右留白加上行自己的左右留白。 */
const ROW_CHROME = 16 + 22;

/** “自动运行”一列怎么排：开着的在前；Windows 服务按自动、延迟、手动、禁用。 */
function autoRank(item: StartupItem): number {
  const order = ["auto", "enabled", "delayed", "manual", "disabled"];
  if (item.startTypes.length > 0 && item.startType) {
    const at = order.indexOf(item.startType);
    return at >= 0 ? at : order.length;
  }
  return item.enabled ? 0 : 4;
}
type OriginFilter = "thirdParty" | "all" | "system";

/** 系统自带的设置页里有启动项管理的系统。 */
const SYSTEM_PAGE = hostPlatform === "macos" || hostPlatform === "windows";

export function StartupView() {
  const t = useT();
  const items = useStartup((state) => state.items);
  const notes = useStartup((state) => state.notes);
  const loading = useStartup((state) => state.loading);
  const error = useStartup((state) => state.error);
  const selectedId = useStartup((state) => state.selectedId);
  const selectedIds = useStartup((state) => state.selectedIds);
  const select = useStartup((state) => state.select);
  const setSelection = useStartup((state) => state.setSelection);
  const load = useStartup((state) => state.load);
  const route = useApp((state) => state.route);
  const [sort, setSort] = useState<{ column: SortColumn; asc: boolean }>({ column: "name", asc: true });
  const [frame, frameWidth] = useWidth<HTMLDivElement>();
  const tab = usePrefs((state) => state.startupTab);
  const setPrefs = usePrefs((state) => state.set);
  const search = useApp((state) => state.search);
  const attempt = useApp((state) => state.attempt);
  const [status, setStatus] = useState<StatusFilter>("all");
  const [origin, setOrigin] = useState<OriginFilter>("thirdParty");

  // 第一次进来时列一遍；之后靠右上角的按钮和窗口回到前台时刷新
  useEffect(() => {
    if (useStartup.getState().items === null) void load();
  }, [load]);

  const byOrigin = useMemo(
    () => (items ?? []).filter((item) => (origin === "all" ? true : origin === "system" ? item.system : !item.system)),
    [items, origin],
  );
  const counts = useMemo(
    () => ({ startup: byOrigin.filter((i) => i.kind === "startup").length, service: byOrigin.filter((i) => i.kind === "service").length }),
    [byOrigin],
  );
  const list = useMemo(() => {
    const query = search.trim().toLowerCase();
    const shown = byOrigin.filter(
      (item) =>
        item.kind === tab &&
        (status === "all" ||
          (status === "enabled" && item.enabled) ||
          (status === "disabled" && !item.enabled) ||
          (status === "running" && item.running === true)) &&
        (!query ||
          [item.name, item.label, item.description, item.publisher, item.app, item.command].some((text) => text.toLowerCase().includes(query))),
    );
    const byName = (a: StartupItem, b: StartupItem) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    const running = (item: StartupItem) => (item.running === true ? 0 : item.running === false ? 1 : 2);
    const source = (item: StartupItem) => t(`startup.source.${item.source}` as Key);
    const compare: Record<SortColumn, (a: StartupItem, b: StartupItem) => number> = {
      name: byName,
      source: (a, b) => source(a).localeCompare(source(b)) || byName(a, b),
      status: (a, b) => running(a) - running(b) || byName(a, b),
      auto: (a, b) => autoRank(a) - autoRank(b) || byName(a, b),
    };
    return shown.sort((a, b) => compare[sort.column](a, b) * (sort.asc ? 1 : -1));
  }, [byOrigin, tab, status, search, sort, t]);
  // 第三方的看完了，提一句还有多少系统自带的没列出来
  const hiddenSystem = origin === "thirdParty" ? (items ?? []).filter((item) => item.system && item.kind === tab).length : 0;
  // “自动运行”一列：Windows 服务要放启动类型的菜单，宽一些；别的只有开关
  const defs = tab === "startup" ? STARTUP_COLUMNS : list.some((item) => item.startTypes.length > 0) ? SERVICE_COLUMNS_WIDE : SERVICE_COLUMNS;
  const columns = useTableColumns(tab, defs, frameWidth - ROW_CHROME);
  // 选中的：多选时是 selectedIds，单选时就是 selectedId 那一个
  const chosen = selectedIds.length > 0 ? selectedIds : selectedId ? [selectedId] : [];

  /** 点一行：⌘ / Ctrl 加选或取消，Shift 从上次点的那一行连选到这一行，否则单选。 */
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

  // ⌘ / Ctrl + A 全选当前列出来的，Esc 取消多选。焦点在输入框、或者开着对话框和菜单时不管
  useEffect(() => {
    if (route !== "startup") return;
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable=true]")) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setSelection(
          list.map((item) => item.id),
          selectedId ?? list[0]?.id ?? null,
        );
      } else if (event.key === "Escape" && chosen.length > 1) {
        select(selectedId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [route, list, selectedId, chosen.length, setSelection, select]);

  const header = (column: SortColumn, label: string) => {
    const Arrow = sort.asc ? ArrowUp : ArrowDown;
    return (
      <button
        type="button"
        onClick={() => setSort({ column, asc: sort.column === column ? !sort.asc : true })}
        className={cx("flex min-w-0 items-center gap-1 text-left hover:text-fg-2", column === "auto" && "ml-auto")}
      >
        <span className="truncate">{label}</span>
        {sort.column === column && <Arrow size={12} strokeWidth={2} aria-hidden className="shrink-0" />}
      </button>
    );
  };
  const tabNotes = notes.filter((note) => (tab === "service" ? note === "services_failed" : note !== "services_failed"));
  const filtering = search.trim() !== "" || status !== "all";

  return (
    <div ref={frame} className="@container flex min-h-0 flex-1 flex-col">
      {/* 和别的列表一样分三段：左边是标题（这里是两个分页），正中间是筛选，右边是按钮 */}
      <div className="grid h-11 shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 border-b border-line pr-2 pl-3">
        <div className="flex min-w-0 items-center">
          <Segmented<"startup" | "service">
            value={tab}
            onChange={(value) => setPrefs({ startupTab: value })}
            options={[
              { value: "startup", label: `${t("startup.tab.startup")} ${items ? counts.startup : ""}`.trim() },
              { value: "service", label: `${t("startup.tab.service")} ${items ? counts.service : ""}`.trim() },
            ]}
          />
        </div>
        <div className="flex min-w-0 items-center justify-center gap-0.5">
          <FilterMenu<StatusFilter>
            textClassName="hidden @[600px]:inline"
            label={t("startup.filter.status")}
            value={status}
            onChange={setStatus}
            options={[
              { value: "all", label: t("startup.filter.all"), icon: <ListFilter size={15} strokeWidth={1.75} /> },
              { value: "enabled", label: t("startup.filter.enabled"), icon: <Power size={15} strokeWidth={1.75} /> },
              { value: "disabled", label: t("startup.filter.disabled"), icon: <PowerOff size={15} strokeWidth={1.75} /> },
              { value: "running", label: t("startup.filter.running"), icon: <Play size={15} strokeWidth={1.75} /> },
            ]}
          />
          <FilterMenu<OriginFilter>
            textClassName="hidden @[600px]:inline"
            label={t("startup.origin")}
            value={origin}
            onChange={setOrigin}
            options={[
              { value: "thirdParty", label: t("startup.origin.thirdParty"), icon: <Users size={15} strokeWidth={1.75} /> },
              { value: "all", label: t("startup.origin.all"), icon: <ListFilter size={15} strokeWidth={1.75} /> },
              { value: "system", label: t("startup.origin.system"), icon: <ShieldCheck size={15} strokeWidth={1.75} /> },
            ]}
          />
        </div>
        <div className="flex items-center justify-end gap-0.5">
          <IconButton label={t("startup.refresh")} disabled={loading} onClick={() => void load()}>
            <RefreshCw size={15} strokeWidth={1.75} className={loading ? "animate-spin" : ""} />
          </IconButton>
          {SYSTEM_PAGE && (
            <IconButton label={t("startup.openSystem")} onClick={() => void attempt(() => api.openStartupSettings("loginItems"))}>
              <ExternalLink size={15} strokeWidth={1.75} />
            </IconButton>
          )}
          <DetailToggle />
        </div>
      </div>

      {tabNotes.map((note) => (
        <Notice key={note} note={note} />
      ))}
      {error && <Notice note="list_failed" detail={error} />}

      {items === null ? (
        <EmptyState icon={LoaderCircle} title={t("startup.loading")} />
      ) : list.length === 0 ? (
        <EmptyState
          icon={filtering ? SearchX : tab === "startup" ? Rocket : Cog}
          title={filtering ? t("lib.noMatch") : t(tab === "startup" ? "startup.empty.startup" : "startup.empty.service")}
          action={
            hiddenSystem > 0 && !filtering ? (
              <Button size="sm" onClick={() => setOrigin("all")}>
                {t("startup.systemHidden", { n: hiddenSystem })}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <ColumnsContext.Provider value={columns}>
          <TableHeader
            table={tab}
            defs={defs}
            columns={columns}
            className="pr-5 pl-[56px]"
            first={header("name", t("startup.col.name"))}
            firstLabel={t("startup.col.name")}
            labels={{ source: t("startup.col.source"), status: t("startup.col.status"), auto: t("startup.col.auto") }}
            cells={{
              source: header("source", t("startup.col.source")),
              status: header("status", t("startup.col.status")),
              auto: header("auto", t("startup.col.auto")),
            }}
          />
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3">
            {list.map((item) => (
              <StartupRow
                key={item.id}
                item={item}
                selected={chosen.includes(item.id)}
                group={chosen.length > 1 && chosen.includes(item.id) ? chosen : null}
                onSelect={(event) => clickRow(item.id, event)}
              />
            ))}
            {hiddenSystem > 0 && !filtering && (
              <div className="flex items-center justify-center gap-2 px-3 pt-3 text-[12px] text-fg-3">
                <span>{t("startup.systemHiddenNote", { n: hiddenSystem })}</span>
                <button type="button" className="font-medium text-accent hover:underline" onClick={() => setOrigin("all")}>
                  {t("startup.showSystem")}
                </button>
              </div>
            )}
          </div>
        </ColumnsContext.Provider>
      )}
    </div>
  );
}

/** 列表上方的提示条：列登录项没有权限、超时，或者服务列不出来。 */
function Notice({ note, detail }: { note: string; detail?: string }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const load = useStartup((state) => state.load);
  const key = `startup.note.${note}` as Key;
  return (
    <div className="mx-3 mt-2 flex shrink-0 items-center gap-2 rounded-lg bg-[color-mix(in_srgb,var(--warning)_13%,transparent)] px-3 py-2 text-[12px] leading-relaxed text-warning">
      <AlertTriangle size={15} strokeWidth={1.75} aria-hidden className="shrink-0" />
      <span className="min-w-0 flex-1">
        {t(key)}
        {detail && <span className="opacity-80"> {detail}</span>}
      </span>
      {note === "login_items_denied" && (
        <Button size="sm" onClick={() => void attempt(() => api.openStartupSettings("automation"))}>
          {t("startup.openAutomation")}
        </Button>
      )}
      {note !== "login_items_denied" && (
        <Button size="sm" onClick={() => void load()}>
          {t("startup.refresh")}
        </Button>
      )}
    </div>
  );
}

/** 运行状态：绿点是正在运行，灰点是没在运行；不知道的（登录项、注册表启动项）画一条短线。 */
function RunState({ item }: { item: StartupItem }) {
  const t = useT();
  if (item.running === null) return <span className="text-[12px] text-fg-4">—</span>;
  return (
    <span className={cx("flex min-w-0 items-center gap-1.5 text-[12px]", item.running ? "text-fg-2" : "text-fg-3")}>
      <span className={cx("size-1.5 shrink-0 rounded-full", item.running ? "bg-success" : "bg-fg-4")} />
      <span className="truncate">{item.running ? t("startup.running") : t("startup.stopped")}</span>
    </span>
  );
}

/** Windows 服务的启动类型：点开选自动、延迟、手动、禁用。 */
function StartTypeMenu({ item, compact }: { item: StartupItem; compact?: boolean }) {
  const t = useT();
  const busy = useStartup((state) => Boolean(state.busy[item.id]));
  const options: MenuEntry[] = item.startTypes.map((type) => ({
    label: startTypeLabel(type),
    icon: type === item.startType ? CircleDot : Circle,
    onSelect: () => {
      if (type !== item.startType) void startupAct(item, `type:${type}` as StartupAction);
    },
  }));
  return (
    <Dropdown items={options}>
      <button
        type="button"
        disabled={busy}
        title={t("startup.fact.startType")}
        aria-label={`${t("startup.fact.startType")} · ${startTypeLabel(item.startType ?? "")}`}
        onClick={(event) => event.stopPropagation()}
        className={cx(
          "inline-flex h-6 max-w-full items-center gap-1 rounded-md px-2 text-[12px] whitespace-nowrap transition-colors outline-none",
          item.startType === "disabled" ? "text-fg-3" : "text-fg-2",
          "hover:bg-active data-[state=open]:bg-active",
          compact ? "" : "border border-line-strong",
        )}
      >
        <span className="truncate">{startTypeLabel(item.startType ?? "")}</span>
        <ChevronDown size={12} strokeWidth={2} aria-hidden className="shrink-0 opacity-50" />
      </button>
    </Dropdown>
  );
}

/** “自动运行”一列：Windows 服务是启动类型的菜单，别的是开关；改不了的写出原因（systemd 的 static、masked）。 */
function AutoControl({ item }: { item: StartupItem }) {
  const t = useT();
  const busy = useStartup((state) => state.busy[item.id]);
  if (busy) return <LoaderCircle size={16} strokeWidth={1.75} className="animate-spin text-fg-3" aria-label={t("startup.working")} />;
  if (item.startTypes.length > 0) return <StartTypeMenu item={item} compact />;
  if (!item.canToggle) return <span className="truncate text-[12px] text-fg-3">{item.startType ? startTypeLabel(item.startType) : "—"}</span>;
  return (
    <span onClick={(event) => event.stopPropagation()} className="flex">
      <Switch
        checked={item.enabled}
        label={item.enabled ? t("startup.disable") : t("startup.enable")}
        onChange={(on) => void startupAct(item, on ? "enable" : "disable")}
      />
    </span>
  );
}

/** 多选时的右键菜单：一起让 AI 解释（凑成一批问，不是一项一项问）。 */
function batchMenu(group: string[], canAi: boolean, t: ReturnType<typeof useT>): MenuEntry[] {
  const items = () => {
    const all = useStartup.getState().items ?? [];
    return group.flatMap((id) => all.filter((item) => item.id === id));
  };
  const explained = group.filter((id) => useStartup.getState().explanations[id]).length;
  return [
    ...(canAi
      ? [
          { label: t("startup.explainMany", { n: group.length }), icon: Sparkles, onSelect: () => void explainMany(items()) },
          ...(explained > 0 ? [{ label: t("startup.explainAgainMany", { n: group.length }), icon: RotateCw, onSelect: () => void explainMany(items(), true) }] : []),
        ]
      : [{ label: t("startup.aiSetup"), icon: Sparkles, onSelect: () => useApp.getState().patch({ settingsOpen: true }) }]),
    { label: t("batch.clear"), icon: CheckSquare, separated: true, onSelect: () => useStartup.getState().select(null) },
  ];
}

/** 右键菜单：启用或停用、启动或停止、打开所在位置、复制命令、让 AI 解释。右键的是多选里的一项时，作用于全部选中的。 */
function useStartupMenu(item: StartupItem, group: string[] | null): MenuEntry[] {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const busy = useStartup((state) => Boolean(state.busy[item.id]));
  const canAi = useApp((state) => aiUsable(state.library?.settings.ai, state.aiKeySaved));
  if (group) return batchMenu(group, canAi, t);
  const entries: MenuEntry[] = [];
  if (item.startTypes.length > 0) {
    item.startTypes.forEach((type, index) =>
      entries.push({
        label: startTypeLabel(type),
        icon: type === item.startType ? CircleDot : Circle,
        disabled: busy,
        separated: index === 0 && entries.length > 0,
        onSelect: () => {
          if (type !== item.startType) void startupAct(item, `type:${type}` as StartupAction);
        },
      }),
    );
  } else if (item.canToggle) {
    entries.push(
      item.enabled
        ? { label: t("startup.disable"), icon: PowerOff, disabled: busy, onSelect: () => void startupAct(item, "disable") }
        : { label: t("startup.enable"), icon: Power, disabled: busy, onSelect: () => void startupAct(item, "enable") },
    );
  }
  if (item.canControl) {
    const separated = entries.length > 0;
    if (item.running) {
      entries.push({ label: t("startup.stop"), icon: Square, disabled: busy, separated, onSelect: () => void startupAct(item, "stop") });
      entries.push({ label: t("startup.restart"), icon: RotateCw, disabled: busy, onSelect: () => void startupAct(item, "restart") });
    } else {
      entries.push({ label: t("startup.start"), icon: Play, disabled: busy || blockedStart(item), separated, onSelect: () => void startupAct(item, "start") });
    }
  }
  entries.push({ label: t("startup.reveal"), icon: FolderOpen, separated: entries.length > 0, onSelect: () => void attempt(() => api.revealStartup(item.id)) });
  if (item.command) {
    entries.push({
      label: t("startup.copyCommand"),
      icon: Copy,
      onSelect: () => void navigator.clipboard.writeText(item.command).then(() => notify(t("startup.copied"))),
    });
  }
  if (canAi) {
    entries.push({ label: t("startup.explain"), icon: Sparkles, separated: true, onSelect: () => {
      useStartup.getState().select(item.id);
      void explainStartup(item);
    } });
  }
  return entries;
}

function StartupRow({
  item,
  selected,
  group,
  onSelect,
}: {
  item: StartupItem;
  selected: boolean;
  group: string[] | null;
  onSelect: (event: { metaKey: boolean; ctrlKey: boolean; shiftKey: boolean }) => void;
}) {
  const t = useT();
  const menu = useStartupMenu(item, group);
  const columns = useContext(ColumnsContext) as Columns<StartupColumn>;
  const show = (key: StartupColumn) => columns.visible.includes(key);
  const explanation = useStartup((state) => state.explanations[item.id]?.text);
  const explaining = useStartup((state) => Boolean(state.explaining[item.id]));
  const select = useStartup((state) => state.select);
  const line = subtitle(item, explanation);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        onClick={onSelect}
        // 右键的不在多选里时，先单选它，菜单作用于它自己
        onContextMenu={() => !group && !selected && select(item.id)}
        onKeyDown={(event) => event.target === event.currentTarget && event.key === "Enter" && onSelect(event)}
        style={{ gridTemplateColumns: columns.template }}
        className={cx("group grid min-h-[50px] items-center gap-x-3 rounded-md py-1.5 pr-3 pl-2.5 transition-colors", selected ? "bg-active" : "hover:bg-hover")}
      >
        <div className="flex min-w-0 items-center gap-2.5">
          <span className={cx("flex", !item.enabled && "opacity-50 grayscale")}>
            <ItemIcon item={item} size={28} />
          </span>
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-1.5">
              <span className={cx("truncate text-[13px] font-medium", !item.enabled && "text-fg-3")}>{item.name}</span>
              {item.system && <Chip className="h-[18px] px-1">{t("startup.system")}</Chip>}
              {explaining && <LoaderCircle size={12} strokeWidth={2} className="shrink-0 animate-spin text-accent" aria-label={t("startup.explaining")} />}
            </div>
            <div className={cx("truncate text-[12px]", explanation && line === explanation ? "text-fg-2" : "text-fg-3")} title={line}>
              {line}
            </div>
          </div>
        </div>
        {show("source") && <div className="truncate text-[12px] text-fg-3">{t(`startup.source.${item.source}` as Key)}</div>}
        {show("status") && <RunState item={item} />}
        {show("auto") && (
          <div className="flex min-w-0 justify-end">
            <AutoControl item={item} />
          </div>
        )}
      </div>
    </ContextMenu>
  );
}

// ── 详情栏 ───────────────────────────────────────────────────

export function StartupDetail() {
  const many = useStartup((state) => state.selectedIds.length > 1);
  return many ? <StartupBatch /> : <StartupSingle />;
}

/** 多选时的详情栏：一起让 AI 解释，下面列出每一项和它的解释，一处看完。 */
function StartupBatch() {
  const t = useT();
  const ids = useStartup((state) => state.selectedIds);
  const all = useStartup((state) => state.items);
  const explanations = useStartup((state) => state.explanations);
  const explaining = useStartup((state) => state.explaining);
  const progress = useStartup((state) => state.explainProgress);
  const select = useStartup((state) => state.select);
  const canAi = useApp((state) => aiUsable(state.library?.settings.ai, state.aiKeySaved));
  const patch = useApp((state) => state.patch);
  // 不能在选择器里拼新数组：每次都是新的，界面会一直重画
  const items = useMemo(() => ids.flatMap((id) => (all ?? []).filter((item) => item.id === id)), [ids, all]);
  const busy = items.some((item) => explaining[item.id]);
  const done = items.filter((item) => explanations[item.id]).length;
  return (
    <div className="flex h-full flex-col">
      <div className="shrink-0 px-4 pt-4">
        <div className="flex items-center gap-2">
          <CheckSquare size={18} strokeWidth={1.75} aria-hidden className="text-accent" />
          <span className="text-[15px] font-semibold">{t("startup.selected", { n: items.length })}</span>
        </div>
        <p className="mt-1.5 mb-0 text-[12px] leading-relaxed text-fg-3">{t("batch.keys")}</p>
        {canAi ? (
          <div className="mt-3 flex gap-2">
            <Button variant="primary" className="flex-1" busy={busy} onClick={() => void explainMany(items)}>
              {!busy && <Sparkles size={14} strokeWidth={1.75} aria-hidden />}
              {busy && progress ? t("startup.explainProgress", { done: progress.done, total: progress.total }) : t("startup.explainMany", { n: items.length })}
            </Button>
            {done > 0 && (
              <IconButton label={t("startup.explainAgainMany", { n: items.length })} disabled={busy} onClick={() => void explainMany(items, true)}>
                <RotateCw size={15} strokeWidth={1.75} />
              </IconButton>
            )}
          </div>
        ) : (
          <p className="mt-2 mb-0 text-[12px] leading-relaxed text-fg-3">
            {t("startup.aiHint")}{" "}
            <button type="button" className="font-medium text-accent hover:underline" onClick={() => patch({ settingsOpen: true })}>
              {t("startup.aiSetup")}
            </button>
          </p>
        )}
      </div>
      <div className="mt-3 min-h-0 flex-1 overflow-y-auto border-t border-line">
        {items.map((item) => {
          const text = explanations[item.id]?.text || item.description;
          return (
            <div key={item.id} className="flex gap-2.5 border-b border-line px-4 py-2.5 last:border-b-0">
              <ItemIcon item={item} size={22} />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-1.5">
                  <span className="truncate text-[13px] font-medium">{item.name}</span>
                  {explaining[item.id] && <LoaderCircle size={12} strokeWidth={2} className="shrink-0 animate-spin text-accent" aria-hidden />}
                </div>
                <p className={cx("selectable m-0 mt-0.5 text-[12px] leading-relaxed", text ? "text-fg-2" : "text-fg-4")}>{text || t("startup.notExplained")}</p>
              </div>
            </div>
          );
        })}
      </div>
      <div className="shrink-0 border-t border-line p-3">
        <Button variant="ghost" className="w-full" onClick={() => select(null)}>
          {t("batch.clear")}
        </Button>
      </div>
    </div>
  );
}

function StartupSingle() {
  const t = useT();
  const item = useStartup((state) => state.items?.find((i) => i.id === state.selectedId));
  const busy = useStartup((state) => (item ? state.busy[item.id] : undefined));
  const explanation = useStartup((state) => (item ? state.explanations[item.id] : undefined));
  const explaining = useStartup((state) => (item ? Boolean(state.explaining[item.id]) : false));
  const canAi = useApp((state) => aiUsable(state.library?.settings.ai, state.aiKeySaved));
  const attempt = useApp((state) => state.attempt);
  const patch = useApp((state) => state.patch);
  const language = useLanguage();

  // 以前让 AI 解释过的，选中时直接拿出来（只读缓存，不问 AI）
  useEffect(() => {
    if (item) void loadExplanation(item);
    // 只在换了一项时取
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item?.id]);

  if (!item) return <EmptyState icon={MousePointerClick} title={t("startup.select")} />;

  const when = triggerText(item);
  const hintKey = (item.canToggle || item.startTypes.length > 0 ? `startup.hint.${item.source}` : "startup.hint.locked") as Key;

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-3">
          <ItemIcon item={item} size={44} />
          <div className="min-w-0">
            <div className="selectable truncate text-[15px] font-semibold" title={item.name}>
              {item.name}
            </div>
            <div className="truncate text-[12px] text-fg-2">{item.publisher || (item.app && t("startup.belongsTo", { app: item.app })) || t(`startup.source.${item.source}` as Key)}</div>
          </div>
        </div>
        <div className="mt-2.5 flex flex-wrap gap-1">
          <Chip>{t(`startup.source.${item.source}` as Key)}</Chip>
          <Chip>{t(`startup.scope.${item.scope}` as Key)}</Chip>
          {item.system && <Chip>{t("startup.system")}</Chip>}
          {item.needsAdmin && <Chip>{t("startup.admin")}</Chip>}
        </div>

        {/* 会不会自动运行：开关，或者 Windows 服务的启动类型 */}
        <div className="mt-4 rounded-lg border border-line px-3 py-2.5">
          <div className="flex items-center gap-3">
            <span className="min-w-0 flex-1 text-[13px] font-medium">
              {item.startTypes.length > 0 ? t("startup.fact.startType") : t(item.kind === "service" ? "startup.autoStart.service" : "startup.autoStart.startup")}
            </span>
            {busy ? (
              <LoaderCircle size={16} strokeWidth={1.75} className="animate-spin text-fg-3" aria-label={t("startup.working")} />
            ) : item.startTypes.length > 0 ? (
              <StartTypeMenu item={item} />
            ) : item.canToggle ? (
              <Switch
                checked={item.enabled}
                label={item.enabled ? t("startup.disable") : t("startup.enable")}
                onChange={(on) => void startupAct(item, on ? "enable" : "disable")}
              />
            ) : (
              <span className="text-[12px] text-fg-3">{item.startType ? startTypeLabel(item.startType) : "—"}</span>
            )}
          </div>
          <p className="mt-1.5 mb-0 text-[12px] leading-relaxed text-fg-3">{t(hintKey, { type: item.startType ? startTypeLabel(item.startType) : "" })}</p>
        </div>

        {/* 说明：系统或程序自带的；没有时可以让 AI 解释 */}
        <section className="mt-4">
          <h3 className="m-0 mb-1.5 text-[12px] font-medium text-fg-3">{t("startup.description")}</h3>
          {item.description ? (
            <p className="selectable m-0 text-[13px] leading-relaxed">{item.description}</p>
          ) : (
            <p className="m-0 text-[13px] leading-relaxed text-fg-3">
              {item.app ? t("startup.belongsToLong", { app: item.app }) : t("startup.noDescription")}
            </p>
          )}
          {explanation && (
            <div className="mt-2.5 rounded-lg bg-accent-soft px-3 py-2">
              <div className="mb-1 flex items-center gap-1 text-[12px] font-medium text-accent">
                <Sparkles size={13} strokeWidth={1.75} aria-hidden />
                {t("startup.aiLabel")}
                <span className="ml-auto truncate font-normal opacity-70" title={explanation.model}>
                  {formatDate(explanation.at, language)}
                </span>
              </div>
              <p className="selectable m-0 text-[13px] leading-relaxed">{explanation.text}</p>
            </div>
          )}
          {canAi ? (
            // 有解释了再点是“重新解释”：一定重新问 AI；没有时先看缓存
            <Button size="sm" variant="ghost" className="mt-2 -ml-2 text-accent" busy={explaining} onClick={() => void explainStartup(item, Boolean(explanation))}>
              {!explaining && <Sparkles size={13} strokeWidth={1.75} aria-hidden />}
              {explaining ? t("startup.explaining") : explanation ? t("startup.explainAgain") : t("startup.explain")}
            </Button>
          ) : (
            !item.description && (
              <p className="mt-1.5 mb-0 text-[12px] leading-relaxed text-fg-3">
                {t("startup.aiHint")}{" "}
                <button type="button" className="font-medium text-accent hover:underline" onClick={() => patch({ settingsOpen: true })}>
                  {t("startup.aiSetup")}
                </button>
              </p>
            )
          )}
        </section>

        <dl className="mt-4 mb-0">
          <Fact label={t("startup.fact.state")}>
            {item.running === null ? (
              <span className="text-fg-3">{item.enabled ? t("startup.filter.enabled") : t("startup.filter.disabled")}</span>
            ) : (
              <span className="flex items-center gap-1.5">
                <RunState item={item} />
                {item.pid !== null && <span className="tabular text-[12px] text-fg-3">{t("startup.pid", { pid: item.pid })}</span>}
              </span>
            )}
          </Fact>
          {when.length > 0 && <Fact label={t("startup.fact.when")}>{when.join(t("common.listSeparator"))}</Fact>}
          {item.startTypes.length === 0 && item.startType && item.kind === "service" && (
            <Fact label={t("startup.fact.startType")}>{startTypeLabel(item.startType)}</Fact>
          )}
          {item.account && <Fact label={t("startup.fact.account")}>{item.account}</Fact>}
          {item.label !== item.name && (
            <Fact label={t("startup.fact.label")}>
              <span className="selectable font-mono text-[12px] break-all text-fg-2">{item.label}</span>
            </Fact>
          )}
          {item.command && (
            <Fact label={t("startup.fact.command")}>
              <span className="selectable font-mono text-[12px] break-all text-fg-2">{item.command}</span>
            </Fact>
          )}
          {item.location && item.location !== item.command && (
            <Fact label={t("startup.fact.location")}>
              <span className="selectable font-mono text-[12px] break-all text-fg-2">{item.location}</span>
            </Fact>
          )}
        </dl>
      </div>

      <div className="flex shrink-0 gap-2 border-t border-line p-3">
        <Button className="flex-1" onClick={() => void attempt(() => api.revealStartup(item.id))}>
          <FolderOpen size={14} strokeWidth={1.75} aria-hidden />
          {t("startup.reveal")}
        </Button>
        {item.canControl &&
          (item.running ? (
            <>
              <Button className="flex-1" disabled={Boolean(busy)} busy={busy === "stop"} onClick={() => void startupAct(item, "stop")}>
                <Square size={13} strokeWidth={1.75} aria-hidden />
                {t("startup.stop")}
              </Button>
              <IconButton label={t("startup.restart")} disabled={Boolean(busy)} onClick={() => void startupAct(item, "restart")}>
                <RotateCw size={15} strokeWidth={1.75} className={busy === "restart" ? "animate-spin" : ""} />
              </IconButton>
            </>
          ) : (
            <Button
              className="flex-1"
              disabled={Boolean(busy) || blockedStart(item)}
              title={blockedStart(item) ? t("startup.enableFirst") : undefined}
              busy={busy === "start"}
              onClick={() => void startupAct(item, "start")}
            >
              <Play size={13} strokeWidth={1.75} aria-hidden />
              {t("startup.start")}
            </Button>
          ))}
      </div>
    </div>
  );
}

/** 停用了的 launchd 任务加载不了，要先启用才能启动。别的（Windows 服务、systemd）停用了也能手动启动。 */
function blockedStart(item: StartupItem): boolean {
  return !item.enabled && (item.source === "launchAgent" || item.source === "launchDaemon");
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3 border-b border-line py-2 text-[13px] last:border-b-0">
      <dt className="w-[72px] shrink-0 text-[12px] leading-[19px] text-fg-3">{label}</dt>
      <dd className="m-0 min-w-0 flex-1">{children}</dd>
    </div>
  );
}
