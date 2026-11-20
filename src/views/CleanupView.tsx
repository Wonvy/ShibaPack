// 清理：缓存、临时文件、日志、散落的安装包。按类别分组，勾选后清理；右边是详情栏。
//
// 打开时只读地扫一遍。安全档（删了会自己重新生成）默认勾上，直接删除；别的默认不勾，移到回收站。
// 要先关掉的程序（浏览器、编辑器）在运行时那一项不能勾，可以在这里帮忙正常退出它。

import {
  AlertTriangle,
  AppWindow,
  BrushCleaning,
  Check,
  ChevronRight,
  CodeXml,
  ExternalLink,
  EyeOff,
  FolderCog,
  FolderOpen,
  Globe,
  History,
  Import,
  LoaderCircle,
  Minus,
  MonitorCog,
  MousePointerClick,
  Package,
  RefreshCw,
  SearchX,
  SlidersHorizontal,
  Ban,
  RotateCw,
  Sparkles,
  Trash2,
  X,
  type LucideIcon,
} from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { SoftwareIcon } from "../components/SoftwareIcon";
import { Button, Chip, ContextMenu, EmptyState, FilterMenu, IconButton, type MenuEntry, Switch, cx } from "../components/ui";
import { api, isMac } from "../lib/api";
import { aiUsable } from "../lib/ai";
import {
  CATEGORIES,
  askClean,
  cancelScan,
  canClean,
  explainCleanup,
  regrowthPerDay,
  rescanIfStale,
  importLoose,
  isChecked,
  lastCleaned,
  pickedItems,
  pickedSize,
  quitApp,
  setChecked,
  textOf,
  useCleanup,
} from "../lib/cleanup";
import type { Key } from "../lib/i18n";
import { formatDate, formatSize } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { useApp, useLanguage, useT } from "../lib/store";
import type { CleanCategory, CleanEntry, CleanItem, CleanRisk } from "../lib/types";

export const CATEGORY_ICON: Record<CleanCategory, LucideIcon> = {
  system: MonitorCog,
  browser: Globe,
  apps: AppWindow,
  development: CodeXml,
  installers: Package,
  privacy: EyeOff,
  trash: Trash2,
  custom: FolderCog,
};

/** 这一项的图标：属于某个装着的应用时用它的图标，否则是类别的图标。 */
export function CleanIcon({ item, size }: { item: CleanItem; size: number }) {
  const app = useApp((state) => (item.app ? state.installed?.find((a) => a.name.toLowerCase() === item.app?.toLowerCase()) : undefined));
  if (app?.icon) return <SoftwareIcon name={app.name} icon={app.icon} size={size} />;
  const Icon = CATEGORY_ICON[item.category];
  return (
    <span className="flex shrink-0 items-center justify-center rounded-[22%] bg-hover text-fg-2" style={{ width: size, height: size }}>
      <Icon size={Math.round(size * 0.55)} strokeWidth={1.75} aria-hidden />
    </span>
  );
}

/** 勾选框。`mixed` 是一组里勾了一部分。 */
function Tick({ checked, disabled, label, onChange }: { checked: boolean | "mixed"; disabled?: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onChange(checked !== true);
      }}
      className={cx(
        "flex size-4 shrink-0 items-center justify-center rounded-[4px] border transition-colors disabled:opacity-40",
        checked ? "border-accent-strong bg-accent-strong text-white" : "border-line-strong bg-bg",
      )}
    >
      {checked === true && <Check size={12} strokeWidth={3} aria-hidden />}
      {checked === "mixed" && <Minus size={12} strokeWidth={3} aria-hidden />}
    </button>
  );
}

const NO_SKIPS: number[] = [];

function sizeText(item: CleanItem, picked?: number): string {
  if (item.size === null) return "—";
  return formatSize(picked ?? item.size);
}

export function CleanupView({ detailToggle }: { detailToggle: ReactNode }) {
  const t = useT();
  const language = useLanguage();
  const items = useCleanup((state) => state.items);
  const scanning = useCleanup((state) => state.scanning);
  const cleaning = useCleanup((state) => state.cleaning);
  const progress = useCleanup((state) => state.progress);
  const error = useCleanup((state) => state.error);
  const skips = useCleanup((state) => state.skips);
  const scan = useCleanup((state) => state.scan);
  const scanProgress = useCleanup((state) => state.scanProgress);
  const partial = useCleanup((state) => state.partial);
  const overrides = usePrefs((state) => state.cleanupChecked);
  const collapsed = usePrefs((state) => state.cleanupCollapsed);
  const setPrefs = usePrefs((state) => state.set);
  const search = useApp((state) => state.search);
  const [category, setCategory] = useState<"all" | CleanCategory>("all");
  const [risk, setRisk] = useState<"all" | CleanRisk>("all");

  // 第一次进来时扫一遍；之后回到这一页或窗口回到前台时，结果放得太久了（10 分钟）就重新扫
  useEffect(() => {
    if (useCleanup.getState().items === null) void useCleanup.getState().scan();
    else rescanIfStale();
    const onFocus = () => document.visibilityState !== "hidden" && rescanIfStale();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, []);

  const all = useMemo(() => items ?? [], [items]);
  const shown = useMemo(() => {
    const query = search.trim().toLowerCase();
    return all.filter(
      (item) =>
        (category === "all" || item.category === category) &&
        (risk === "all" || item.risk === risk) &&
        (!query || [textOf(item.name, language), item.app ?? "", ...item.roots].some((text) => text.toLowerCase().includes(query))),
    );
  }, [all, category, risk, search, language]);
  const groups = useMemo(
    () =>
      CATEGORIES.map((c) => ({
        category: c,
        items: shown.filter((item) => item.category === c).sort((a, b) => (b.size ?? -1) - (a.size ?? -1)),
      })).filter((group) => group.items.length > 0),
    [shown],
  );
  const total = all.reduce((sum, item) => sum + (item.size ?? 0), 0);
  // 清理的是列出来的、勾着的
  const picked = pickedItems(shown, overrides);
  const pickedTotal = picked.reduce((sum, item) => sum + pickedSize(item, skips), 0);
  const recommended = pickedItems(all, overrides).filter((item) => item.risk === "safe");
  const recommendedTotal = recommended.reduce((sum, item) => sum + pickedSize(item, skips), 0);
  const filtering = category !== "all" || risk !== "all" || search.trim() !== "";
  const busy = scanning || cleaning;

  const toggleGroup = (c: CleanCategory) =>
    setPrefs({ cleanupCollapsed: collapsed.includes(c) ? collapsed.filter((x) => x !== c) : [...collapsed, c] });

  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      {/* 和别的列表一样分三段：左边是合计，正中间是筛选，右边是按钮 */}
      <div className="grid h-17 shrink-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-x-2 border-b border-line py-1 pr-2 pl-3 @[440px]:h-11 @[440px]:grid-cols-[auto_minmax(0,1fr)_auto] @[440px]:py-0">
        <div className="col-start-1 row-start-1 flex min-w-0 items-center gap-2 text-[13px] font-medium">
          {scanning ? (
            <>
              <LoaderCircle size={14} strokeWidth={2} className="shrink-0 animate-spin text-fg-3" aria-hidden />
              <span className="truncate text-fg-2">
                {scanProgress && scanProgress.total > 0
                  ? t("cleanup.scanningProgress", { done: scanProgress.done, total: scanProgress.total, n: all.length })
                  : t("cleanup.scanningFound", { n: all.length })}
              </span>
            </>
          ) : (
            <span className="truncate">{t("cleanup.total", { size: formatSize(total) })}</span>
          )}
        </div>
        <div className="col-span-2 col-start-1 row-start-2 flex min-w-0 items-center justify-center gap-0.5 @[440px]:col-span-1 @[440px]:col-start-2 @[440px]:row-start-1">
          <FilterMenu<"all" | CleanCategory>
            textClassName="hidden @[600px]:inline"
            label={t("cleanup.filter.category")}
            value={category}
            onChange={setCategory}
            options={[
              { value: "all", label: t("cleanup.filter.allCategories") },
              ...CATEGORIES.map((c) => {
                const Icon = CATEGORY_ICON[c];
                return { value: c, label: t(`cleanup.category.${c}` as Key), icon: <Icon size={15} strokeWidth={1.75} /> };
              }),
            ]}
          />
          <FilterMenu<"all" | CleanRisk>
            label={t("cleanup.filter.risk")}
            value={risk}
            onChange={setRisk}
            options={[
              { value: "all", label: t("cleanup.filter.allRisks") },
              { value: "safe", label: t("cleanup.risk.safe") },
              { value: "recoverable", label: t("cleanup.risk.recoverable") },
              { value: "high", label: t("cleanup.risk.high") },
            ]}
          />
        </div>
        <div className="col-start-2 row-start-1 flex items-center justify-end gap-0.5 @[440px]:col-start-3">
          {scanning ? (
            <IconButton label={t("cleanup.cancelScan")} onClick={cancelScan}>
              <X size={15} strokeWidth={1.75} />
            </IconButton>
          ) : (
            <IconButton label={t("cleanup.scan")} disabled={busy} onClick={() => void scan()}>
              <RefreshCw size={15} strokeWidth={1.75} />
            </IconButton>
          )}
          <IconButton label={t("cleanup.history")} onClick={() => useCleanup.setState({ historyOpen: true })}>
            <History size={15} strokeWidth={1.75} />
          </IconButton>
          <IconButton label={t("cleanup.options")} onClick={() => useCleanup.setState({ optionsOpen: true })}>
            <SlidersHorizontal size={15} strokeWidth={1.75} />
          </IconButton>
          {detailToggle}
        </div>
      </div>

      {error && (
        <div className="mx-3 mt-2 flex shrink-0 items-center gap-2 rounded-lg bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] px-3 py-2 text-[12px] text-danger">
          <AlertTriangle size={15} strokeWidth={1.75} aria-hidden className="shrink-0" />
          <span className="min-w-0 flex-1">{error}</span>
        </div>
      )}

      {partial && !scanning && (
        <div className="mx-3 mt-2 flex shrink-0 items-center gap-2 rounded-lg bg-[color-mix(in_srgb,var(--warning)_13%,transparent)] px-3 py-2 text-[12px] text-warning">
          <AlertTriangle size={15} strokeWidth={1.75} aria-hidden className="shrink-0" />
          <span className="min-w-0 flex-1">{t("cleanup.partial")}</span>
          <Button size="sm" disabled={busy} onClick={() => void scan()}>
            {t("cleanup.scan")}
          </Button>
        </div>
      )}

      {items === null || (scanning && all.length === 0) ? (
        <EmptyState icon={LoaderCircle} title={t("cleanup.loading")} />
      ) : groups.length === 0 ? (
        <EmptyState icon={filtering ? SearchX : BrushCleaning} title={filtering ? t("cleanup.noMatch") : t("cleanup.empty")} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pt-2 pb-3">
          {!filtering && recommended.length > 0 && (
            <div className="mx-1 mb-2 flex items-center gap-3 rounded-lg border border-line px-3 py-2.5">
              <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent">
                <BrushCleaning size={16} strokeWidth={1.75} aria-hidden />
              </span>
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium">{t("cleanup.recommend.title", { size: formatSize(recommendedTotal) })}</div>
                <div className="truncate text-[12px] text-fg-3">{t("cleanup.recommend.body", { n: recommended.length })}</div>
              </div>
              <Button variant="primary" disabled={busy} onClick={() => askClean(recommended.map((item) => item.id))}>
                {t("cleanup.recommend.button")}
              </Button>
            </div>
          )}
          {groups.map((group) => (
            <Group key={group.category} category={group.category} items={group.items} open={!collapsed.includes(group.category)} onToggle={() => toggleGroup(group.category)} />
          ))}
        </div>
      )}

      {/* 底部：选了多少，清理按钮 */}
      <div className="flex h-12 shrink-0 items-center gap-3 border-t border-line pr-3 pl-4">
        <span className="min-w-0 flex-1 truncate text-[12px] text-fg-2">
          {cleaning && progress?.waitingAdmin
            ? t("cleanup.waitingAdmin")
            : cleaning && progress
            ? t("cleanup.cleaning", { done: progress.done, total: progress.total })
            : picked.length > 0
              ? t("cleanup.selected", { n: picked.length, size: formatSize(pickedTotal) })
              : t("cleanup.noneSelected")}
        </span>
        <Button variant="primary" busy={cleaning} disabled={busy || picked.length === 0} onClick={() => askClean(picked.map((item) => item.id))}>
          {!cleaning && <BrushCleaning size={14} strokeWidth={1.75} aria-hidden />}
          {t("cleanup.clean")}
        </Button>
      </div>
    </div>
  );
}

/** 一个类别：标题行（折叠、整组勾选、合计），下面是各项。 */
function Group({ category, items, open, onToggle }: { category: CleanCategory; items: CleanItem[]; open: boolean; onToggle: () => void }) {
  const t = useT();
  const overrides = usePrefs((state) => state.cleanupChecked);
  const Icon = CATEGORY_ICON[category];
  const selectable = items.filter(canClean);
  const checkedCount = selectable.filter((item) => isChecked(item, overrides)).length;
  const state: boolean | "mixed" = checkedCount === 0 ? false : checkedCount === selectable.length ? true : "mixed";
  const size = items.reduce((sum, item) => sum + (item.size ?? 0), 0);
  // 回收站读不到大小时，合计也写“—”，不写成 0
  const known = items.some((item) => item.size !== null);
  const label = t(`cleanup.category.${category}` as Key);
  return (
    <section className="mb-1">
      <div
        role="button"
        tabIndex={0}
        aria-expanded={open}
        onClick={onToggle}
        onKeyDown={(event) => event.target === event.currentTarget && event.key === "Enter" && onToggle()}
        className="flex h-8 items-center gap-2.5 rounded-md pr-3 pl-2.5 text-[12px] text-fg-3 hover:bg-hover"
      >
        <Tick checked={state} disabled={selectable.length === 0} label={state === true ? t("cleanup.uncheckAll") : t("cleanup.checkAll")} onChange={(on) => setChecked(selectable, on)} />
        <Icon size={15} strokeWidth={1.75} aria-hidden className="shrink-0" />
        <span className="font-medium text-fg-2">{label}</span>
        <span className="tabular">{items.length}</span>
        <ChevronRight size={13} strokeWidth={2} aria-hidden className={cx("shrink-0 transition-transform", open && "rotate-90")} />
        <span className="ml-auto tabular">{known ? formatSize(size) : "—"}</span>
      </div>
      {open && items.map((item) => <Row key={item.id} item={item} />)}
    </section>
  );
}

function Row({ item }: { item: CleanItem }) {
  const t = useT();
  const language = useLanguage();
  const selected = useCleanup((state) => state.selectedId === item.id);
  const quitting = useCleanup((state) => Boolean(state.quitting[item.id]));
  const skips = useCleanup((state) => state.skips);
  const select = useCleanup((state) => state.select);
  const overrides = usePrefs((state) => state.cleanupChecked);
  const attempt = useApp((state) => state.attempt);
  const checked = isChecked(item, overrides) && canClean(item);
  const app = item.app ?? textOf(item.name, language);
  const menu: MenuEntry[] = [
    canClean(item)
      ? { label: checked ? t("cleanup.uncheck") : t("cleanup.check"), icon: Check, onSelect: () => setChecked([item], !checked) }
      : { label: t("cleanup.quit", { app }), icon: ExternalLink, disabled: !item.canQuit || quitting, onSelect: () => void quitApp(item) },
    ...(item.roots.length > 0 ? [{ label: t("cleanup.reveal"), icon: FolderOpen, onSelect: () => void attempt(() => api.cleanupReveal(item.id, null)) }] : []),
    ...(item.importable ? [{ label: t("cleanup.import"), icon: Import, onSelect: () => importLoose(item) }] : []),
  ];
  const subtitle = item.running ? t("cleanup.runningShort", { app }) : item.note ? t(`cleanup.note.${item.note}` as Key) : textOf(item.about, language);
  return (
    <ContextMenu items={menu}>
      <div
        role="button"
        tabIndex={0}
        aria-pressed={selected}
        onClick={() => select(item.id)}
        onContextMenu={() => select(item.id)}
        onKeyDown={(event) => event.target === event.currentTarget && event.key === "Enter" && select(item.id)}
        className={cx("flex min-h-[50px] items-center gap-2.5 rounded-md py-1.5 pr-3 pl-2.5 transition-colors", selected ? "bg-active" : "hover:bg-hover")}
      >
        <Tick checked={checked} disabled={!canClean(item)} label={checked ? t("cleanup.uncheck") : t("cleanup.check")} onChange={(on) => setChecked([item], on)} />
        <span className={cx("flex", item.running && "opacity-50 grayscale")}>
          <CleanIcon item={item} size={28} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5">
            <span className={cx("truncate text-[13px] font-medium", item.running && "text-fg-3")}>{textOf(item.name, language)}</span>
            {item.risk !== "safe" && <Chip className="h-[18px] px-1">{t(`cleanup.risk.${item.risk}` as Key)}</Chip>}
            {item.admin && <Chip className="h-[18px] px-1">{t("cleanup.admin")}</Chip>}
          </div>
          <div className={cx("flex min-w-0 items-center gap-1.5 text-[12px]", item.running ? "text-warning" : "text-fg-3")}>
            <span className="truncate">{subtitle}</span>
            {item.running && item.canQuit && (
              <button
                type="button"
                disabled={quitting}
                className="shrink-0 font-medium text-accent hover:underline disabled:opacity-50"
                onClick={(event) => {
                  event.stopPropagation();
                  void quitApp(item);
                }}
              >
                {quitting ? t("cleanup.quitting") : t("cleanup.quitShort")}
              </button>
            )}
          </div>
        </div>
        <span className={cx("tabular shrink-0 text-[13px]", checked ? "text-fg" : "text-fg-3")}>{sizeText(item, checked ? pickedSize(item, skips) : undefined)}</span>
      </div>
    </ContextMenu>
  );
}

// ── 详情栏 ───────────────────────────────────────────────────

export function CleanupDetail() {
  const t = useT();
  const language = useLanguage();
  const item = useCleanup((state) => state.items?.find((i) => i.id === state.selectedId));
  // 不能在选择器里给新的空数组：每次都不一样，界面会一直重画
  const skips = useCleanup((state) => (item ? (state.skips[item.id] ?? NO_SKIPS) : NO_SKIPS));
  const quitting = useCleanup((state) => (item ? Boolean(state.quitting[item.id]) : false));
  const busy = useCleanup((state) => state.scanning || state.cleaning);
  const overrides = usePrefs((state) => state.cleanupChecked);
  const attempt = useApp((state) => state.attempt);
  const history = useCleanup((state) => state.history);
  const keepCookies = useCleanup((state) => state.keepCookies);

  if (!item) return <EmptyState icon={MousePointerClick} title={t("cleanup.select")} />;

  const app = item.app ?? textOf(item.name, language);
  const checked = isChecked(item, overrides);
  const picked = item.entryCount - skips.length;

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-3">
          <CleanIcon item={item} size={44} />
          <div className="min-w-0">
            <div className="selectable truncate text-[15px] font-semibold" title={textOf(item.name, language)}>
              {textOf(item.name, language)}
            </div>
            <div className="truncate text-[12px] text-fg-2">{t(`cleanup.category.${item.category}` as Key)}</div>
          </div>
        </div>
        <div className="mt-2.5 flex flex-wrap gap-1">
          <Chip>{t(`cleanup.risk.${item.risk}` as Key)}</Chip>
          {!item.tool && <Chip>{t(`cleanup.action.${item.action}` as Key)}</Chip>}
          {item.admin && <Chip>{t("cleanup.admin")}</Chip>}
        </div>

        {/* 多大、清不清这一项 */}
        <div className="mt-4 rounded-lg border border-line px-3 py-2.5">
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1">
              <div className="tabular text-[18px] font-semibold">{item.size === null ? t("cleanup.unknownSize") : formatSize(pickedSize(item, { [item.id]: skips }))}</div>
              <div className="text-[12px] text-fg-3">
                {item.size !== null && `${t("cleanup.filesCount", { n: item.files.toLocaleString() })} · `}
                {item.size === null && item.files > 0 && `${t("cleanup.itemsCount", { n: item.files.toLocaleString() })} · `}
                {t(`cleanup.riskHint.${item.risk}` as Key)}
              </div>
            </div>
            <Switch checked={checked && canClean(item)} disabled={!canClean(item)} label={checked ? t("cleanup.uncheck") : t("cleanup.check")} onChange={(on) => setChecked([item], on)} />
          </div>
        </div>

        {item.running && (
          // 按钮放在文字下面：详情栏窄，放在右边会把文字挤成一小条
          <div className="mt-3 rounded-lg bg-[color-mix(in_srgb,var(--warning)_13%,transparent)] px-3 py-2 text-[12px] leading-relaxed text-warning">
            <div className="flex items-start gap-2">
              <AlertTriangle size={15} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
              <span className="min-w-0 flex-1">{t("cleanup.running", { app })}</span>
            </div>
            {item.canQuit && (
              <Button size="sm" className="mt-2 ml-[23px]" busy={quitting} onClick={() => void quitApp(item)}>
                {quitting ? t("cleanup.quitting") : t("cleanup.quit", { app })}
              </Button>
            )}
          </div>
        )}
        {item.note && (
          <div className="mt-3 rounded-lg bg-hover px-3 py-2 text-[12px] leading-relaxed text-fg-2">
            <div>{t(`cleanup.note.${item.note}` as Key)}</div>
            {item.note === "full_disk_access" && isMac && (
              <Button size="sm" className="mt-2" onClick={() => void attempt(() => api.openStartupSettings("fullDiskAccess"))}>
                {t("cleanup.openFullDisk")}
              </Button>
            )}
          </div>
        )}
        {item.cookies && (
          <div className="mt-3 rounded-lg bg-hover px-3 py-2 text-[12px] leading-relaxed text-fg-2">
            <div>{keepCookies.length > 0 ? t("cleanup.keepCookies.some", { n: keepCookies.length }) : t("cleanup.keepCookies.none")}</div>
            <Button size="sm" className="mt-2" onClick={() => useCleanup.setState({ optionsOpen: true })}>
              {t("cleanup.keepCookies.edit")}
            </Button>
          </div>
        )}

        <section className="mt-4">
          <h3 className="m-0 mb-1.5 text-[12px] font-medium text-fg-3">{t("cleanup.about")}</h3>
          <p className="selectable m-0 text-[13px] leading-relaxed">{textOf(item.about, language)}</p>
        </section>
        <section className="mt-3">
          <h3 className="m-0 mb-1.5 text-[12px] font-medium text-fg-3">{t("cleanup.effect")}</h3>
          <p className="selectable m-0 text-[13px] leading-relaxed">{textOf(item.effect, language)}</p>
        </section>

        <AiNote item={item} />

        <dl className="mt-4 mb-0">
          <Regrowth item={item} last={lastCleaned(item.id, history)} perDay={regrowthPerDay(item.id, history)} />
          {item.modified !== null && <Fact label={t("cleanup.fact.modified")}>{formatDate(new Date(item.modified).toISOString(), language, true)}</Fact>}
          {item.roots.length > 0 && (
            <Fact label={t("cleanup.fact.where")}>
              {item.roots.map((root) => (
                <div key={root} className="selectable font-mono text-[12px] break-all text-fg-2">
                  {root}
                </div>
              ))}
            </Fact>
          )}
          {item.evidence && (
            <Fact label={t("cleanup.fact.evidence")}>
              <button type="button" className="text-left text-[12px] break-all text-accent hover:underline" onClick={() => void attempt(() => api.openUrl(item.evidence ?? ""))}>
                {item.evidence.replace(/^https:\/\//, "")}
              </button>
            </Fact>
          )}
        </dl>

        {item.entries.length > 0 && <Entries item={item} skips={skips} picked={picked} />}
      </div>

      <div className="flex shrink-0 gap-2 border-t border-line p-3">
        {item.roots.length > 0 && (
          <IconButton label={t("cleanup.reveal")} onClick={() => void attempt(() => api.cleanupReveal(item.id, null))}>
            <FolderOpen size={15} strokeWidth={1.75} />
          </IconButton>
        )}
        {item.importable && (
          <Button className="flex-1" disabled={picked === 0} onClick={() => importLoose(item)}>
            <Import size={14} strokeWidth={1.75} aria-hidden />
            {t("cleanup.import")}
          </Button>
        )}
        {item.tool ? (
          <Button variant="primary" className="flex-1" onClick={() => void attempt(() => api.openStartupSettings("storage"))}>
            <ExternalLink size={14} strokeWidth={1.75} aria-hidden />
            {t("cleanup.openStorage")}
          </Button>
        ) : (
          <Button variant="primary" className="flex-1" disabled={busy || !canClean(item) || (item.entryCount > 0 && picked === 0)} onClick={() => askClean([item.id])}>
            <BrushCleaning size={14} strokeWidth={1.75} aria-hidden />
            {t("cleanup.cleanThis")}
          </Button>
        )}
      </div>
    </div>
  );
}

/** 要清理的条目：可以一条一条取消；右键可以打开所在位置，或者加进排除列表。 */
function Entries({ item, skips, picked }: { item: CleanItem; skips: number[]; picked: number }) {
  const t = useT();
  const language = useLanguage();
  const toggleEntry = useCleanup((state) => state.toggleEntry);
  const setEntries = useCleanup((state) => state.setEntries);
  const allOn = skips.length === 0;
  return (
    <section className="mt-4">
      <div className="mb-1 flex items-center gap-2">
        <h3 className="m-0 flex-1 text-[12px] font-medium text-fg-3">
          {t("cleanup.contents")} <span className="tabular font-normal">{t("cleanup.contentsCount", { picked, n: item.entryCount })}</span>
        </h3>
        <button
          type="button"
          className="text-[12px] text-accent hover:underline"
          onClick={() => setEntries(item.id, allOn ? item.entries.map((e) => e.index) : [])}
        >
          {allOn ? t("cleanup.uncheckAll") : t("cleanup.checkAll")}
        </button>
      </div>
      <div className="rounded-lg border border-line">
        {item.entries.map((entry) => (
          <EntryRow key={entry.index} item={item} entry={entry} on={!skips.includes(entry.index)} onToggle={() => toggleEntry(item.id, entry.index)} language={language} />
        ))}
      </div>
      {item.entryCount > item.entries.length && (
        <p className="mt-1.5 mb-0 text-[12px] text-fg-3">{t("cleanup.moreEntries", { n: item.entryCount - item.entries.length })}</p>
      )}
    </section>
  );
}

function EntryRow({ item, entry, on, onToggle, language }: { item: CleanItem; entry: CleanEntry; on: boolean; onToggle: () => void; language: string }) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const parts = entry.path.split(/[\\/]/);
  const name = parts.pop() ?? entry.path;
  const parent = parts.slice(-2).join("/");
  const library = entry.library;
  const detail = library
    ? library.same
      ? t("cleanup.lib.same", { software: library.software, version: library.versions[0] ?? "" })
      : t("cleanup.lib.known", { software: library.software, versions: library.versions.join(t("common.listSeparator")) })
    : entry.modified
      ? `${parent} · ${formatDate(new Date(entry.modified).toISOString(), language)}`
      : parent;
  const exclude = async () => {
    const options = await api.cleanupOptions();
    if (options.exclude.includes(entry.path)) return;
    const ok = await attempt(() => api.cleanupSetOptions({ ...options, exclude: [...options.exclude, entry.path] }));
    if (ok === undefined) return;
    notify(t("cleanup.excluded"));
    void useCleanup.getState().scan();
  };
  return (
    <ContextMenu
      items={[
        { label: t("cleanup.reveal"), icon: FolderOpen, onSelect: () => void attempt(() => api.cleanupReveal(item.id, entry.index)) },
        { label: t("cleanup.excludeEntry"), icon: SlidersHorizontal, onSelect: () => void exclude() },
      ]}
    >
      <div className="flex items-center gap-2.5 border-b border-line px-2.5 py-1.5 last:border-b-0">
        <Tick checked={on} label={on ? t("cleanup.uncheck") : t("cleanup.check")} onChange={onToggle} />
        <div className="min-w-0 flex-1">
          <div className={cx("selectable truncate text-[12px]", on ? "text-fg" : "text-fg-3 line-through")} title={entry.path}>
            {name}
          </div>
          <div className={cx("truncate text-[12px]", library?.same ? "text-success" : "text-fg-3")}>{detail}</div>
        </div>
        <span className="tabular shrink-0 text-[12px] text-fg-2">{formatSize(entry.size)}</span>
        <IconButton label={t("cleanup.excludeEntry")} className="size-6 text-fg-3" onClick={() => void exclude()}>
          <Ban size={13} strokeWidth={1.75} />
        </IconButton>
      </div>
    </ContextMenu>
  );
}

/** 上次清理是什么时候、清掉了多少、之后又长了多少。 */
function Regrowth({ item, last, perDay }: { item: CleanItem; last: { at: string; size: number } | null; perDay: number | null }) {
  const t = useT();
  if (!last) return null;
  // 清过两次以上才算得出多快长回来：一天不到 1 MB 的按周说
  const rate = perDay === null ? null : perDay * 7 >= 1024 * 1024 ? t("cleanup.regrowthRate", { size: formatSize(perDay * 7) }) : t("cleanup.regrowthSlow");
  const days = Math.floor((Date.now() - new Date(last.at).getTime()) / 86_400_000);
  const when = days <= 0 ? t("cleanup.days.today") : t("cleanup.days.n", { n: days });
  const cleaned = formatSize(last.size);
  const text = item.size === null || item.size === 0 ? t("cleanup.regrowthGone", { when, cleaned }) : t("cleanup.regrowth", { when, cleaned, now: formatSize(item.size) });
  return (
    <Fact label={t("cleanup.fact.lastCleaned")}>
      {text}
      {rate && <div className="text-[12px] text-fg-3">{rate}</div>}
    </Fact>
  );
}

/** AI 的解释：这些文件是什么、删了会怎样。有解释就显示；配了 AI 模型时可以问、可以重新问。 */
function AiNote({ item }: { item: CleanItem }) {
  const t = useT();
  const language = useLanguage();
  const explanation = useCleanup((state) => state.explanations[item.id]);
  const explaining = useCleanup((state) => Boolean(state.explaining[item.id]));
  const canAi = useApp((state) => aiUsable(state.library?.settings.ai, state.aiKeySaved));
  if (!explanation && !canAi) return null;
  return (
    <section className="mt-3">
      {explanation && (
        <div className="rounded-lg bg-accent-soft px-3 py-2">
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
      {canAi && (
        <Button size="sm" variant="ghost" className="mt-1 -ml-2 text-accent" busy={explaining} onClick={() => void explainCleanup(item, Boolean(explanation))}>
          {!explaining && (explanation ? <RotateCw size={13} strokeWidth={1.75} aria-hidden /> : <Sparkles size={13} strokeWidth={1.75} aria-hidden />)}
          {explaining ? t("startup.explaining") : explanation ? t("startup.explainAgain") : t("startup.explain")}
        </Button>
      )}
    </section>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3 border-b border-line py-2 text-[13px] last:border-b-0">
      <dt className="w-[72px] shrink-0 text-[12px] leading-[19px] text-fg-3">{label}</dt>
      <dd className="m-0 min-w-0 flex-1">{children}</dd>
    </div>
  );
}
