// 窗口的外框：顶栏、搜索框、窗口按钮、底栏、拖拽手柄、详情栏开关、导入进度条、提示条。

import {
  ArrowUp,
  CloudDownload,
  FilePlus,
  FolderInput,
  FolderSearch,
  Globe,
  Minus,
  PanelRightClose,
  PanelRightOpen,
  Plus,
  ScanSearch,
  Square,
  X,
} from "lucide-react";
import { type PointerEvent, useEffect, useRef, useState } from "react";
import { api, isMac, isTauri, pickFiles, pickFolder, windowControls } from "../lib/api";
import { fileCount, formatSize, softwareSize } from "../lib/library";
import { TOPBAR, usePrefs } from "../lib/prefs";
import { useApp, useT } from "../lib/store";
import { dismissUpdate, finishUpdate, openUpdateDialog, startUpdate } from "../lib/updater";
import { SearchBox } from "./SearchBox";
import { Button, Dropdown, ICON, IconButton, cx } from "./ui";

export function Logo({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1024 1024" aria-hidden className="pointer-events-none shrink-0">
      <rect x="64" y="64" width="896" height="896" rx="220" fill="var(--accent-strong)" />
      <g fill="none" stroke="#fff" strokeWidth="64" strokeLinejoin="round" strokeLinecap="round">
        <path d="M512 248 L760 380 L760 644 L512 776 L264 644 L264 380 Z" />
        <path d="M264 380 L512 512 L760 380 M512 512 L512 776" />
      </g>
    </svg>
  );
}

/**
 * 顶栏：左边是名字，中间是搜索，右边是“添加”。空白处可以拖动窗口。`bare` 用在引导页。
 * 检查在线更新在侧栏“有更新”那一行上，整理散落文件在设置的“存储”页里。
 * 高度可以拖下沿来调（40–72px，默认 52px）。macOS 的红黄绿按钮由系统摆放，不跟着高度动。
 */
export function TopBar({ bare = false }: { bare?: boolean }) {
  const t = useT();
  const importPaths = useApp((state) => state.importPaths);
  const patch = useApp((state) => state.patch);
  const height = usePrefs((state) => state.topBarHeight);
  // macOS 的红黄绿按钮在左上角，给它们让出位置；其他系统的窗口按钮自己画在右边
  const nativeLights = isTauri && isMac;
  const ownControls = isTauri && !isMac;

  const importFiles = async () => importPaths(await pickFiles());
  const importFolder = async (mode?: "managed" | "indexed") => {
    const folder = await pickFolder();
    if (folder) await importPaths([folder], mode);
  };

  return (
    <header
      data-tauri-drag-region
      className="relative flex shrink-0 items-center gap-1 border-b border-line bg-side pr-2"
      style={{ height, paddingLeft: nativeLights ? 80 : 12 }}
    >
      <Logo />
      <span className="pointer-events-none ml-1 text-[13px] font-semibold">ShibaPack</span>
      <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
      {!bare && <SearchBox />}
      <div data-tauri-drag-region className="h-full min-w-4 flex-1" />
      {!bare && (
        // 把软件收进仓库的方式都在这一个菜单里
        <Dropdown
          items={[
            { label: t("add.files"), icon: FilePlus, onSelect: () => void importFiles() },
            { label: t("add.folder"), icon: FolderInput, onSelect: () => void importFolder() },
            { label: t("scan.folder"), icon: ScanSearch, separated: true, onSelect: () => void importFolder("indexed") },
            { label: t("add.online"), icon: Globe, separated: true, onSelect: () => patch({ sourceDialog: { softwareId: null } }) },
          ]}
        >
          <Button variant="primary" className="ml-1">
            <Plus size={15} strokeWidth={2} aria-hidden />
            {t("app.add")}
          </Button>
        </Dropdown>
      )}
      {ownControls && <WindowControls />}
      <HeightHandle />
    </header>
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
      <div className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 transition-colors group-hover:h-0.5 group-hover:bg-accent group-active:h-0.5 group-active:bg-accent" />
    </div>
  );
}

/** 展开或收起右边的详情栏。放在列表工具栏最右边；图标本身表示开关状态，不再用强调色。 */
export function DetailToggle() {
  const t = useT();
  const open = usePrefs((state) => state.detailOpen);
  const set = usePrefs((state) => state.set);
  const label = open ? t("layout.hideDetail") : t("layout.showDetail");
  return (
    <IconButton label={label} aria-pressed={open} onClick={() => set({ detailOpen: !open })}>
      {open ? <PanelRightClose size={16} strokeWidth={1.75} /> : <PanelRightOpen size={16} strokeWidth={1.75} />}
    </IconButton>
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
 * 给了 `onCollapse` 时，拖得比 `collapseBelow` 还窄就收起这一栏，宽度回到开始拖之前，展开时还是原来那么宽。
 */
export function ResizeHandle({
  edge,
  width,
  onResize,
  collapseBelow = 0,
  onCollapse,
}: {
  edge: "left" | "right";
  width: number;
  onResize: (width: number) => void;
  collapseBelow?: number;
  onCollapse?: () => void;
}) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    start.current = { x: event.clientX, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const delta = event.clientX - start.current.x;
    const next = start.current.width + (edge === "left" ? delta : -delta);
    if (onCollapse && next < collapseBelow) {
      onResize(start.current.width);
      start.current = null;
      onCollapse();
      return;
    }
    onResize(next);
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
      className="group relative z-10 -mx-0.5 w-1 shrink-0 cursor-col-resize"
    >
      <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-line transition-colors group-hover:w-0.5 group-hover:bg-accent group-active:w-0.5 group-active:bg-accent" />
    </div>
  );
}

/** 中间栏顶部的导入进度：3px 进度条加一行文字。 */
export function ImportProgress() {
  const t = useT();
  const progress = useApp((state) => state.progress);
  const queued = useApp((state) => state.importQueue.length);
  const cancelImport = useApp((state) => state.cancelImport);
  const [stopping, setStopping] = useState(false);
  // 新的一批开始时，“正在停止”的状态要清掉
  useEffect(() => {
    if (!progress) setStopping(false);
  }, [progress]);
  if (!progress) return null;
  const known = progress.phase === "import" && progress.total > 0;
  const text = known
    ? t("import.progress", { current: progress.current, total: progress.total, file: progress.file })
    : t("import.collecting");
  return (
    <div className="shrink-0">
      <div className="h-[3px] overflow-hidden bg-line">
        {known ? (
          <div
            className="h-full bg-accent transition-[width] duration-200"
            style={{ width: `${Math.round((progress.current / progress.total) * 100)}%` }}
          />
        ) : (
          <div className="h-full w-2/5 animate-slide bg-accent" />
        )}
      </div>
      <div className="flex items-center gap-2 px-3 pt-1 text-[12px] text-fg-3">
        <span className="min-w-0 flex-1 truncate">
          {stopping ? t("import.stopping") : text}
          {queued > 0 && ` · ${t("import.queuedCount", { n: queued })}`}
        </span>
        {/* 停在两个文件之间：已经导入的保留 */}
        <button
          type="button"
          disabled={stopping}
          onClick={() => {
            setStopping(true);
            cancelImport();
          }}
          className="shrink-0 rounded px-1.5 py-0.5 text-fg-2 hover:bg-hover hover:text-fg disabled:opacity-50"
        >
          {t("import.cancel")}
        </button>
      </div>
    </div>
  );
}

export function StatusBar() {
  const t = useT();
  const library = useApp((state) => state.library);
  const status = useApp((state) => state.status);
  const update = useApp((state) => state.appUpdate);
  const importing = useApp((state) => state.progress !== null);
  const checking = useApp((state) => state.checking);
  // 正在下载的那个文件（排队中的不算）
  const downloading = useApp((state) => Object.values(state.downloads).find((d) => d.received > 0) ?? Object.values(state.downloads)[0]);
  const attempt = useApp((state) => state.attempt);
  if (!library) return null;
  const busy = importing || checking !== null || downloading !== undefined;
  const percent = downloading?.total ? `${Math.round((downloading.received / downloading.total) * 100)}%` : "";
  const activity = downloading
    ? t("download.progress", { name: downloading.name, percent })
    : checking
      ? t("updates.checking", { current: checking.current, total: checking.total, name: checking.name })
      : importing
        ? t("status.busy")
        : t("status.ready");
  const files = library.software.reduce((sum, s) => sum + fileCount(s), 0);
  const size = library.software.reduce((sum, s) => sum + softwareSize(s), 0);
  // ShibaPack 自己有新版本时，版本号右边带一个小箭头，点了重新弹出更新提示
  const appUpdate = ["available", "downloading", "ready", "installed"].includes(update.phase);
  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-line bg-side px-3 text-[12px] text-fg-3">
      <span className="flex min-w-0 items-center gap-1.5">
        <span className={cx("size-1.5 shrink-0 rounded-full", busy ? "bg-accent" : "bg-success")} />
        <span className="max-w-[360px] truncate">{activity}</span>
      </span>
      <span className="tabular shrink-0">{t("status.software", { n: library.software.length })}</span>
      <span className="tabular shrink-0">{t("status.files", { n: files })}</span>
      <span className="tabular shrink-0">{formatSize(size)}</span>
      <button
        type="button"
        title={t("storage.open")}
        onClick={() => void attempt(() => api.openRepoFolder())}
        className="ml-auto flex min-w-0 items-center gap-1.5 rounded px-1 hover:text-fg-2"
      >
        <FolderSearch size={13} strokeWidth={1.75} aria-hidden className="shrink-0" />
        <span className="truncate">{status?.repoRoot}</span>
      </button>
      <span className="h-3 w-px shrink-0 bg-line-strong" />
      {/* 版本号：一直带下划线，点了打开“软件更新”窗口；有新版本时换成强调色，右边一个小箭头 */}
      <button
        type="button"
        title={appUpdate ? t("update.appAvailable", { version: update.version }) : t("update.title")}
        onClick={openUpdateDialog}
        className={cx(
          "tabular flex shrink-0 items-center gap-0.5 rounded px-1 underline decoration-1 underline-offset-[3px] transition-colors",
          appUpdate ? "font-medium text-accent decoration-accent/60 hover:bg-hover" : "decoration-fg-4 hover:text-fg-2 hover:decoration-fg-3",
        )}
      >
        v{status?.version}
        {appUpdate && <ArrowUp size={12} strokeWidth={2.25} aria-hidden />}
      </button>
    </footer>
  );
}

/** 右下角的更新提示：有新版本、正在下载、等待安装或重启时出现。 */
export function UpdateCard() {
  const t = useT();
  const update = useApp((state) => state.appUpdate);
  const shown = ["available", "downloading", "ready", "installed"].includes(update.phase) && !update.dismissed;
  if (!shown) return null;
  const vars = { version: update.version };
  const percent = update.progress === null ? null : Math.round(update.progress * 100);
  return (
    <div role="status" className="fixed right-3 bottom-10 z-40 w-[300px] animate-pop rounded-xl bg-panel p-3 shadow-pop">
      <div className="flex items-center gap-2 text-[13px] font-medium">
        <CloudDownload size={15} strokeWidth={1.75} aria-hidden className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1">
          {update.phase === "available" && t("update.available", vars)}
          {update.phase === "downloading" && t("update.downloading", vars)}
          {update.phase === "ready" && t("update.ready", vars)}
          {update.phase === "installed" && t("update.installed", vars)}
        </span>
        {update.phase === "downloading" && percent !== null && <span className="tabular shrink-0 text-[12px] text-fg-3">{percent}%</span>}
      </div>
      {update.phase === "downloading" ? (
        <div className="mt-2.5 h-1 overflow-hidden rounded-full bg-line">
          {percent === null ? (
            <div className="h-full w-2/5 animate-slide bg-accent" />
          ) : (
            <div className="h-full bg-accent transition-[width] duration-200" style={{ width: `${percent}%` }} />
          )}
        </div>
      ) : (
        <div className="mt-2.5 flex justify-end gap-2">
          <Button variant="ghost" className="mr-auto" onClick={openUpdateDialog}>
            {t("update.notesLink")}
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
    <div className="pointer-events-none fixed inset-x-0 bottom-10 z-50 flex flex-col items-center gap-1.5 px-6">
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
