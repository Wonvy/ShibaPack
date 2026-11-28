// 窗口的外框：顶栏、搜索框、窗口按钮、底栏、拖拽手柄、详情栏开关、导入进度条、提示条。

import {
  ChevronLeft,
  ChevronRight,
  CircleArrowUp,
  CloudDownload,
  FilePlus,
  FolderInput,
  FolderSearch,
  Globe,
  Minus,
  Plus,
  RotateCcw,
  ScanSearch,
  Square,
  X,
  TriangleAlert,
} from "lucide-react";
import { type PointerEvent, useMemo, useRef, useState } from "react";
import { api, isMac, isTauri, pickFiles, pickFolder, windowControls } from "../lib/api";
import { pathTitle, usePaths } from "../lib/paths";
import { useCleanup } from "../lib/cleanup";
import { useDisk } from "../lib/disk";
import { categoryPath, fileCount, filterSoftware, formatSize, softwareSize } from "../lib/library";
import { TOPBAR, usePrefs } from "../lib/prefs";
import { useApp, useT } from "../lib/store";
import type { Progress } from "../lib/types";
import { dismissUpdate, finishUpdate, openUpdateDialog, startUpdate } from "../lib/updater";
import { useHoverCard } from "./HoverCard";
import { SearchBox } from "./SearchBox";
import { Button, ContextMenu, Dropdown, ICON, cx } from "./ui";

export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden className="pointer-events-none shrink-0">
      <rect x="64" y="64" width="896" height="896" rx="220" fill="var(--accent)" />
      <g fill="none" stroke="var(--on-accent)" strokeWidth="64" strokeLinejoin="round" strokeLinecap="round">
        <path d="M512 248 L760 380 L760 644 L512 776 L264 644 L264 380 Z" />
        <path d="M264 380 L512 512 L760 380 M512 512 L512 776" />
      </g>
    </svg>
  );
}

/**
 * 顶栏：搜索框与列表栏居中对齐，空白处可以拖动窗口。引导页没有列表栏。
 * 检查在线更新在侧栏“有更新”那一行上，整理散落文件在设置的“存储”页里。
 * 高度可以拖下沿来调（40–72px，默认 52px）。macOS 的红黄绿按钮由系统摆放，不跟着高度动。
 */
export function TopBar({ listBounds }: { listBounds?: { left: number; right: number; windowWidth: number } }) {
  const height = usePrefs((state) => state.topBarHeight);
  // macOS 的红黄绿按钮在左上角，给它们让出位置；其他系统的窗口按钮自己画在右边
  const nativeLights = isTauri && isMac;
  const ownControls = isTauri && !isMac;

  const { left = 0, right = 0, windowWidth = 0 } = listBounds ?? {};
  const listWidth = windowWidth - left - right;
  // 在列表两侧对称留空，避免搜索框碰到标题或窗口按钮。macOS 不显示标题，只给红黄绿按钮留位置。
  const searchPadding = Math.max(8, nativeLights ? 88 - left : 0, ownControls ? 144 - right : 0);
  const separateSearchRow = Boolean(listBounds && searchPadding > 8 && listWidth - searchPadding * 2 < 180);
  const resultsMaxWidth = Math.max(0, windowWidth - Math.abs(left - right) - 16);

  return (
    <header
      data-tauri-drag-region
      className="relative shrink-0 border-b border-line bg-side"
      style={{ height: height + (separateSearchRow ? 36 : 0) }}
    >
      <div
        data-tauri-drag-region
        className="flex items-center gap-1 pr-2"
        style={{ height: height - 1, paddingLeft: nativeLights ? 80 : 12 }}
      >
        {/* macOS 左上角只留红黄绿按钮，不显示图标和名字 */}
        {!isMac && (
          <>
            <Logo />
            <span className="pointer-events-none ml-1 text-[13px] font-semibold">ShibaPack</span>
          </>
        )}
        <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
        {ownControls && <WindowControls />}
      </div>
      {listBounds && (
        <div
          className="pointer-events-none absolute flex items-center justify-center [&>*]:pointer-events-auto"
          style={{ left, right, top: separateSearchRow ? height : 0, height: separateSearchRow ? 32 : height - 1, paddingInline: separateSearchRow ? 8 : searchPadding }}
        >
          <SearchBox resultsMaxWidth={resultsMaxWidth} />
        </div>
      )}
      <HeightHandle />
    </header>
  );
}

type AddPosition = { x: number; y: number };
const DEFAULT_ADD_POSITION: AddPosition = { x: 1, y: 1 };

export function AddMenuButton({ coveredWidth }: { coveredWidth: number }) {
  const t = useT();
  const patch = useApp((state) => state.patch);
  const savedPosition = usePrefs((state) => state.addButtonPosition);
  const [draft, setDraft] = useState<AddPosition | null>(null);
  const area = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: number; offset: AddPosition; next: AddPosition | null } | null>(null);
  const position = draft ?? savedPosition ?? DEFAULT_ADD_POSITION;
  const label = t("app.add");
  const hover = useHoverCard(label, "label");

  const startDrag = (event: PointerEvent<HTMLButtonElement>) => {
    hover.handlers.onPointerDown();
    if (event.button !== 1 || drag.current) return;
    event.preventDefault();
    const button = event.currentTarget.getBoundingClientRect();
    drag.current = { id: event.pointerId, offset: { x: event.clientX - button.left, y: event.clientY - button.top }, next: null };
    event.currentTarget.setPointerCapture(event.pointerId);
    setDraft(position);
  };
  const moveDrag = (event: PointerEvent<HTMLButtonElement>) => {
    const active = drag.current;
    if (!active || event.pointerId !== active.id) return;
    const bounds = area.current?.getBoundingClientRect();
    if (!bounds) return;
    const button = event.currentTarget;
    const width = bounds.width - button.offsetWidth;
    const height = bounds.height - button.offsetHeight;
    const ratio = (offset: number, range: number) => Math.min(1, Math.max(0, offset / range));
    active.next = {
      x: width > 0 ? ratio(event.clientX - bounds.left - active.offset.x, width) : position.x,
      y: height > 0 ? ratio(event.clientY - bounds.top - active.offset.y, height) : position.y,
    };
    setDraft(active.next);
  };
  const finishDrag = (event: PointerEvent<HTMLButtonElement>) => {
    const active = drag.current;
    if (!active || event.pointerId !== active.id) return;
    drag.current = null;
    if (event.type === "pointerup" && active.next) usePrefs.getState().set({ addButtonPosition: active.next });
    setDraft(null);
  };

  const importFiles = async () => {
    const paths = await pickFiles();
    if (paths.length) patch({ importDialog: { paths } });
  };
  const importFolder = async (mode?: "managed" | "indexed") => {
    const folder = await pickFolder();
    if (folder) patch({ importDialog: { paths: [folder], storage: mode } });
  };

  return (
    <div
      ref={area}
      className="pointer-events-none absolute inset-x-4 top-12 bottom-[calc(var(--bottom-bar-height)+12px)] z-10"
      style={coveredWidth > 0 ? { left: 4, right: coveredWidth + 4 } : undefined}
    >
      <ContextMenu items={[{ label: t("add.resetPosition"), icon: RotateCcw, onSelect: () => usePrefs.getState().set({ addButtonPosition: null }) }]}>
        <div
          className="absolute"
          style={{ left: `${position.x * 100}%`, top: `${position.y * 100}%`, translate: `${-position.x * 100}% ${-position.y * 100}%` }}
        >
          <Dropdown
            side="top"
            items={[
              { label: t("add.files"), icon: FilePlus, onSelect: () => void importFiles() },
              { label: t("add.folder"), icon: FolderInput, onSelect: () => void importFolder() },
              { label: t("scan.folder"), icon: ScanSearch, separated: true, onSelect: () => void importFolder("indexed") },
              { label: t("add.online"), icon: Globe, separated: true, onSelect: () => patch({ sourceDialog: { softwareId: null } }) },
            ]}
          >
            <button
              type="button"
              aria-label={label}
              className={cx(
                "pointer-events-auto flex size-10 items-center justify-center rounded-full bg-accent-fill text-on-accent shadow-pop transition-[filter] hover:brightness-105 data-[state=open]:brightness-105 motion-reduce:transition-none",
                draft && "cursor-grabbing",
              )}
              {...hover.handlers}
              onMouseOver={draft ? undefined : hover.handlers.onMouseOver}
              onPointerDown={startDrag}
              onPointerMove={moveDrag}
              onPointerUp={finishDrag}
              onPointerCancel={finishDrag}
              onLostPointerCapture={finishDrag}
              onAuxClick={(event) => { if (event.button === 1) event.preventDefault(); }}
            >
              <Plus size={21} strokeWidth={2} aria-hidden />
            </button>
          </Dropdown>
        </div>
      </ContextMenu>
      {hover.card}
    </div>
  );
}

/** 顶栏下沿的拖拽条：上下拖改顶栏高度，双击回到默认高度。 */
function HeightHandle() {
  const t = useT();
  const height = usePrefs((state) => state.topBarHeight);
  const set = usePrefs((state) => state.set);
  const start = useRef<{ y: number; height: number } | null>(null);
  const stop = () => {
    start.current = null;
  };
  return (
    <div
      role="separator"
      aria-orientation="horizontal"
      aria-label={t("layout.resizeTopBar")}
      title={t("layout.resizeTopBar")}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        start.current = { y: event.clientY, height };
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (start.current) set({ topBarHeight: start.current.height + event.clientY - start.current.y });
      }}
      onPointerUp={stop}
      onPointerCancel={stop}
      onDoubleClick={() => set({ topBarHeight: TOPBAR.initial })}
      // 骑在顶栏的下边线上，上下各占几个像素，好按住
      className="group absolute inset-x-0 -bottom-[3px] z-20 h-1.5 cursor-row-resize"
    >
      <div className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 transition-colors group-hover:h-0.5 group-hover:bg-accent-fill group-active:h-0.5 group-active:bg-accent-fill" />
    </div>
  );
}

export function DetailToggle({ open, onClick, disabled }: {
  open: boolean;
  onClick: () => void;
  disabled?: boolean;
}) {
  const t = useT();
  const label = t(open ? "layout.hideDetail" : "layout.showDetail");
  const Icon = open ? ChevronRight : ChevronLeft;
  return (
    <span className="flex w-7 shrink-0 items-center justify-center">
      <button
        type="button"
        aria-label={label}
        title={label}
        aria-pressed={open}
        data-detail-toggle=""
        disabled={disabled}
        onClick={onClick}
        className="flex h-7 w-5 cursor-pointer items-center justify-center rounded-md text-fg-2 transition-[width,background-color,color] hover:w-7 hover:bg-hover hover:text-fg focus-visible:w-7 disabled:opacity-50 motion-reduce:transition-none"
      >
        <Icon size={15} strokeWidth={1.75} aria-hidden />
      </button>
    </span>
  );
}

function WindowControls() {
  const t = useT();
  const button = "inline-flex h-7 w-9 items-center justify-center rounded-md text-fg-2 transition-colors hover:bg-hover hover:text-fg";
  return (
    <div className="ml-2 flex items-center border-l border-line pl-2">
      <button type="button" aria-label={t("win.minimize")} title={t("win.minimize")} className={button} onClick={() => void windowControls.minimize()}>
        <Minus {...ICON} />
      </button>
      <button type="button" aria-label={t("win.maximize")} title={t("win.maximize")} className={button} onClick={() => void windowControls.toggleMaximize()}>
        <Square size={13} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        aria-label={t("win.close")}
        title={t("win.close")}
        className={cx(button, "hover:bg-danger hover:text-white")}
        onClick={() => void windowControls.close()}
      >
        <X {...ICON} />
      </button>
    </div>
  );
}

/**
 * 两栏之间的拖拽手柄。`edge` 说明它调整的是左边那一栏还是右边那一栏。
 */
export function ResizeHandle({
  edge,
  width,
  onResize,
  onReset,
}: {
  edge: "left" | "right";
  width: number;
  onResize: (width: number) => void;
  onReset?: () => void;
}) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    start.current = { x: event.clientX, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = start.current;
    if (!drag) return;
    const delta = event.clientX - drag.x;
    onResize(drag.width + (edge === "left" ? delta : -delta));
  };
  const stop = () => {
    start.current = null;
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={stop}
      onPointerCancel={stop}
      onDoubleClick={onReset}
      className="group relative z-10 -mx-0.5 w-1 shrink-0 cursor-col-resize"
    >
      <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-line transition-colors group-hover:w-0.5 group-hover:bg-accent-fill group-active:w-0.5 group-active:bg-accent-fill" />
    </div>
  );
}

/** 导入时在底栏显示进度、当前文件和取消按钮。 */
function ImportProgress({ progress }: { progress: Progress }) {
  const t = useT();
  const queued = useApp((state) => state.importQueue.length);
  const cancelImport = useApp((state) => state.cancelImport);
  const stopping = useApp((state) => state.importCancelled);
  const percent = progress.phase === "import" && progress.total > 0
    ? Math.round((progress.current / progress.total) * 100)
    : undefined;
  const text = percent === undefined
    ? t("import.collecting")
    : t("import.progress", { current: progress.current, total: progress.total, file: progress.file });
  const message = stopping ? t("import.stopping") : text;
  const activity = queued > 0 ? `${message} · ${t("import.queuedCount", { n: queued })}` : message;
  return (
    <>
      <div
        role="progressbar"
        aria-label={t("status.busy")}
        aria-valuenow={percent}
        aria-valuetext={percent === undefined ? undefined : text}
        className="pointer-events-none absolute inset-x-0 -top-px h-0.5 overflow-hidden bg-line"
      >
        {percent === undefined ? (
          <div className="h-full w-2/5 bg-accent-fill motion-safe:animate-slide" />
        ) : (
          <div
            className="h-full bg-accent-fill transition-[width] duration-200"
            style={{ width: `${percent}%` }}
          />
        )}
      </div>
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="min-w-0 truncate text-fg-2" title={activity}>{activity}</span>
        {/* 停在两个文件之间：已经导入的保留 */}
        <button
          type="button"
          disabled={stopping}
          onClick={cancelImport}
          className="shrink-0 rounded px-1.5 py-0.5 text-fg-2 hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          {t("import.cancel")}
        </button>
      </div>
    </>
  );
}

export function StatusBar() {
  const t = useT();
  const library = useApp((state) => state.library);
  const status = useApp((state) => state.status);
  const update = useApp((state) => state.appUpdate);
  const progress = useApp((state) => state.progress);
  const checking = useApp((state) => state.checking);
  // 正在下载的那个文件（排队中的不算）
  const downloading = useApp((state) => Object.values(state.downloads).find((d) => d.received > 0) ?? Object.values(state.downloads)[0]);
  const attempt = useApp((state) => state.attempt);
  const chooseRepo = useApp((state) => state.chooseRepo);
  const changeRepoLabel = t("storage.changeFolder");
  const changeRepoHover = useHoverCard(changeRepoLabel, "label");
  const route = useApp((state) => state.route);
  const cleanupSummary = useCleanup((state) => state.summary);
  const cleanupBusy = useCleanup((state) => state.cleaning || state.scanning);
  if (!library) return null;
  const busy = checking !== null || downloading !== undefined;
  const percent = downloading?.total ? `${Math.round((downloading.received / downloading.total) * 100)}%` : "";
  const activity = downloading
    ? t("download.progress", { name: downloading.name, percent })
    : checking
      ? t("updates.checking", { current: checking.current, total: checking.total, name: checking.name })
      : t("status.ready");
  const files = library.software.reduce((sum, s) => sum + fileCount(s), 0);
  const size = library.software.reduce((sum, s) => sum + softwareSize(s), 0);
  // 已发现新版本时，即使自动安装失败，仍保留更新入口。
  const appUpdate = ["available", "downloading", "ready", "installed"].includes(update.phase) || (update.phase === "error" && update.version !== "");
  return (
    <footer className="relative flex h-(--bottom-bar-height) shrink-0 items-center gap-3 border-t border-line bg-side px-3 text-[12px] text-fg-3">
      {progress ? (
        <ImportProgress progress={progress} />
      ) : (
        route === "disk" ? (
          <DiskStatus />
        ) : route === "paths" ? (
          <PathsStatus />
        ) : route === "cleanup" ? (
          // 清理页：状态栏写选了多少、直接删除和移到回收站各多少
          <span className="flex min-w-0 items-center gap-1.5">
            <span className={cx("size-1.5 shrink-0 rounded-full", cleanupBusy ? "bg-accent-fill" : "bg-success")} />
            <span className="tabular truncate">{cleanupSummary}</span>
          </span>
        ) : (
        <>
          <span className="flex min-w-0 items-center gap-1.5">
            <span className={cx("size-1.5 shrink-0 rounded-full", busy ? "bg-accent-fill" : "bg-success")} />
            <span className="max-w-[360px] truncate">{activity}</span>
          </span>
          {route === "library" ? (
            <LibraryCounts />
          ) : (
            <>
              <span className="tabular shrink-0">{t("status.software", { n: library.software.length })}</span>
              <span className="tabular shrink-0">{t("status.files", { n: files })}</span>
              <span className="tabular shrink-0">{formatSize(size)}</span>
            </>
          )}
        </>
        )
      )}
      <div className={cx("ml-auto flex min-w-6 items-center", progress && "max-w-[28%]")}>
        <button
          type="button"
          aria-label={changeRepoLabel}
          onClick={() => void chooseRepo()}
          className="inline-flex size-6 shrink-0 items-center justify-center rounded transition-colors hover:bg-hover hover:text-fg"
          {...changeRepoHover.handlers}
        >
          <FolderSearch size={13} strokeWidth={1.75} aria-hidden />
        </button>
        {changeRepoHover.card}
        <button
          type="button"
          title={t("storage.open")}
          onClick={() => void attempt(() => api.openRepoFolder())}
          className="min-w-0 truncate rounded px-0.5 hover:text-fg-2"
        >
          {status?.repoRoot}
        </button>
      </div>
      <span className="h-3 w-px shrink-0 bg-line-strong" />
      {/* 点击版本号或箭头都打开“软件更新”窗口。 */}
      <button
        type="button"
        title={appUpdate ? t("update.appAvailable", { version: update.version }) : t("update.title")}
        onClick={openUpdateDialog}
        className={cx(
          "tabular flex shrink-0 items-center gap-1 rounded px-1 underline decoration-1 underline-offset-[3px] transition-colors",
          appUpdate ? "font-medium text-accent decoration-accent/60 hover:bg-hover" : "decoration-fg-4 hover:text-fg-2 hover:decoration-fg-3",
        )}
      >
        v{status?.version}
        {appUpdate && <CircleArrowUp size={15} strokeWidth={2} className="shrink-0 rounded-full bg-accent-soft text-accent" aria-hidden />}
      </button>
    </footer>
  );
}

/** 右下角的更新提示：有新版本、正在下载、等待安装或重启时出现。 */
export function UpdateCard() {
  const t = useT();
  const update = useApp((state) => state.appUpdate);
  // 新版本没能自动装上时也提示一下，点“查看”在更新窗口里看原因和办法
  const failedInstall = update.phase === "error" && update.version !== "";
  const shown = (["available", "downloading", "ready", "installed"].includes(update.phase) || failedInstall) && !update.dismissed;
  if (!shown) return null;
  const vars = { version: update.version };
  const percent = update.progress === null ? null : Math.round(update.progress * 100);
  return (
    <div role="status" className="fixed right-3 bottom-[calc(var(--bottom-bar-height)+68px)] z-40 w-[300px] animate-pop rounded-xl bg-panel p-3 shadow-pop">
      <div className="flex items-center gap-2 text-[13px] font-medium">
        <CloudDownload size={15} strokeWidth={1.75} aria-hidden className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1">
          {update.phase === "available" && t("update.available", vars)}
          {update.phase === "downloading" && t("update.downloading", vars)}
          {update.phase === "ready" && t("update.ready", vars)}
          {update.phase === "installed" && t("update.installed", vars)}
          {failedInstall && t("update.installFailed", vars)}
        </span>
        {update.phase === "downloading" && percent !== null && <span className="tabular shrink-0 text-[12px] text-fg-3">{percent}%</span>}
      </div>
      {update.phase === "downloading" ? (
        <div className="mt-2.5 h-1 overflow-hidden rounded-full bg-line">
          {percent === null ? (
            <div className="h-full w-2/5 animate-slide bg-accent-fill" />
          ) : (
            <div className="h-full bg-accent-fill transition-[width] duration-200" style={{ width: `${percent}%` }} />
          )}
        </div>
      ) : (
        <div className="mt-2.5 flex justify-end gap-2">
          <Button variant="ghost" className="mr-auto" onClick={openUpdateDialog}>
            {failedInstall ? t("update.view") : t("update.notesLink")}
          </Button>
          <Button variant="ghost" onClick={dismissUpdate}>
            {t("update.later")}
          </Button>
          {update.phase === "available" && (
            <Button variant="primary" onClick={() => void startUpdate()}>
              {t("update.install")}
            </Button>
          )}
          {update.phase === "ready" && (
            <Button variant="primary" onClick={() => void finishUpdate()}>
              {t("update.installRestart")}
            </Button>
          )}
          {update.phase === "installed" && (
            <Button variant="primary" onClick={() => void finishUpdate()}>
              {t("update.restart")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}

export function ToastView() {
  const t = useT();
  const toasts = useApp((state) => state.toasts);
  const dismiss = useApp((state) => state.dismissToast);
  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-[calc(var(--bottom-bar-height)+12px)] z-50 flex flex-col items-center gap-1.5 px-6">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.kind === "error" ? "alert" : "status"}
          className={cx(
            "selectable pointer-events-auto flex max-w-[560px] animate-pop items-start gap-2 rounded-lg bg-panel px-3 py-2 text-[13px] shadow-pop",
            toast.kind === "error" ? "text-danger" : "text-fg",
          )}
        >
          <span className="min-w-0">{toast.text}</span>
          {/* 出错的提示留得久，可以先关掉 */}
          {toast.kind === "error" && (
            <button
              type="button"
              aria-label={t("common.close")}
              onClick={() => dismiss(toast.id)}
              className="-mr-1 flex size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
            >
              <X size={13} strokeWidth={2} aria-hidden />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

/** 磁盘分析页的状态栏：扫描进度；扫完后写现在这一层多大、多少个文件、整个位置多大、用了多久、几个文件夹没有权限。 */
/**
 * 软件库的状态栏：跟着左边点的入口（所有、收藏、更新、未分类、某个分类）走，
 * 写这个入口的名字和里面的软件数、安装包数、大小。平台、安装状态、搜索又筛掉一些时写“列出 n / 共 m 个”。多选时加上选了几个。
 */
function LibraryCounts() {
  const t = useT();
  const library = useApp((state) => state.library);
  const scope = useApp((state) => state.scope);
  const platform = useApp((state) => state.platform);
  const installFilter = useApp((state) => state.installFilter);
  const links = useApp((state) => state.links);
  const search = useApp((state) => state.search);
  const selected = useApp((state) => state.selectedIds.length);
  const counts = useMemo(() => {
    if (!library) return null;
    const inScope = filterSoftware(library.software, library.categories, { scope, platform: "all", installFilter: "all", links, search: "" });
    const shown = filterSoftware(inScope, library.categories, { scope: { kind: "all" }, platform, installFilter, links, search });
    return {
      total: inScope.length,
      shown: shown.length,
      files: shown.reduce((sum, s) => sum + fileCount(s), 0),
      size: shown.reduce((sum, s) => sum + softwareSize(s), 0),
    };
  }, [library, scope, platform, installFilter, links, search]);
  if (!library || !counts) return null;
  const title =
    scope.kind === "category"
      ? categoryPath(library.categories, scope.id)
      : scope.kind === "all"
        ? ""
        : t(scope.kind === "starred" ? "nav.starred" : scope.kind === "updates" ? "nav.updates" : "nav.uncategorized");
  const narrowed = counts.shown !== counts.total;
  return (
    <>
      {title && (
        <span className="min-w-0 max-w-[200px] truncate font-medium text-fg-2" title={title}>
          {title}
        </span>
      )}
      <span className="tabular shrink-0">
        {narrowed ? t("status.softwareOf", { n: counts.shown, total: counts.total }) : t("status.software", { n: counts.total })}
      </span>
      <span className="tabular shrink-0">{t("status.files", { n: counts.files })}</span>
      <span className="tabular shrink-0">{formatSize(counts.size)}</span>
      {selected > 1 && <span className="tabular shrink-0">{t("status.selected", { n: selected })}</span>}
    </>
  );
}

/** 路径页的状态栏：在备份或恢复哪一条；平时是收藏了多少、备份过多少、备份占多大，有不在了的也提一句。 */
function PathsStatus() {
  const t = useT();
  const items = useApp((state) => state.library?.paths);
  const states = usePaths((state) => state.states);
  const busy = usePaths((state) => state.busy);
  const selected = usePaths((state) => state.selectedIds.length);
  const list = items ?? [];
  const working = Object.entries(busy)[0];
  const workingItem = working && list.find((item) => item.id === working[0]);
  const activity = workingItem
    ? t(working[1] === "backup" ? "paths.status.backingUp" : "paths.status.restoring", { name: pathTitle(workingItem), n: Object.keys(busy).length })
    : t("status.ready");
  const known = list.map((item) => states?.[item.id]).filter((state) => state !== undefined);
  const backedUp = known.filter((state) => state.backups > 0).length;
  const missing = known.filter((state) => !state.exists).length;
  const size = known.reduce((sum, state) => sum + state.backupSize, 0);
  return (
    <>
      <span className="flex min-w-0 items-center gap-1.5">
        <span className={cx("size-1.5 shrink-0 rounded-full", workingItem ? "bg-accent-fill" : "bg-success")} />
        <span className="max-w-[360px] truncate">{activity}</span>
      </span>
      <span className="tabular shrink-0">{t("paths.status.count", { n: list.length })}</span>
      {states && (
        <>
          <span className="tabular shrink-0">{t("paths.status.backedUp", { n: backedUp })}</span>
          {size > 0 && <span className="tabular shrink-0">{t("paths.status.size", { size: formatSize(size) })}</span>}
          {missing > 0 && <span className="tabular shrink-0 text-warning">{t("paths.status.missing", { n: missing })}</span>}
        </>
      )}
      {selected > 1 && <span className="tabular shrink-0">{t("paths.selected", { n: selected })}</span>}
    </>
  );
}

function DiskStatus() {
  const t = useT();
  const scanning = useDisk((state) => state.scanning);
  const progress = useDisk((state) => state.progress);
  const summary = useDisk((state) => state.summary);
  const view = useDisk((state) => state.view);
  const path = useDisk((state) => state.path);
  const dot = <span className={cx("size-1.5 shrink-0 rounded-full", scanning ? "bg-accent-fill" : "bg-success")} />;
  if (scanning) {
    return (
      <span className="flex min-w-0 items-center gap-1.5">
        {dot}
        <span className="tabular truncate">
          {progress ? t("disk.scanned", { files: progress.files.toLocaleString(), size: formatSize(progress.bytes) }) : t("disk.starting")}
        </span>
      </span>
    );
  }
  if (!summary || !view) {
    return (
      <span className="flex min-w-0 items-center gap-1.5">
        {dot}
        <span className="truncate">{t("disk.status.idle")}</span>
      </span>
    );
  }
  return (
    <>
      <span className="flex min-w-0 items-center gap-1.5">
        {dot}
        <span className="tabular truncate">{t("disk.status.level", { size: formatSize(view.size), files: view.files.toLocaleString() })}</span>
      </span>
      {path.length > 0 && <span className="tabular shrink-0">{t("disk.status.total", { size: formatSize(summary.size) })}</span>}
      <span className="tabular shrink-0">{t("disk.status.time", { seconds: (summary.elapsedMs / 1000).toFixed(1) })}</span>
      {summary.denied > 0 && (
        <span className="flex shrink-0 items-center gap-1 text-warning" title={summary.deniedExamples.join("\n")}>
          <TriangleAlert size={12} strokeWidth={1.75} aria-hidden />
          {t("disk.denied", { n: summary.denied })}
        </span>
      )}
    </>
  );
}
