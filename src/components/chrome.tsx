// 窗口的外框：顶栏、窗口按钮、底栏、拖拽手柄、导入进度条、提示条。

import { CloudDownload, FolderInput, FolderSearch, FolderSync, Globe, Minus, Plus, RefreshCw, ScanSearch, Settings, Square, X } from "lucide-react";
import { type PointerEvent, useRef } from "react";
import { api, isMac, isTauri, pickFiles, pickFolder, windowControls } from "../lib/api";
import { fileCount, formatSize, softwareSize } from "../lib/library";
import { useApp, useT } from "../lib/store";
import { dismissUpdate, finishUpdate, startUpdate } from "../lib/updater";
import { Button, Dropdown, ICON, cx } from "./ui";

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

/** 顶栏。空白处可以拖动窗口。`bare` 用在引导页：只留标题和窗口按钮。 */
export function TopBar({ bare = false }: { bare?: boolean }) {
  const t = useT();
  const importPaths = useApp((state) => state.importPaths);
  const patch = useApp((state) => state.patch);
  const checkUpdates = useApp((state) => state.checkUpdates);
  const tidyRoot = useApp((state) => state.tidyRoot);
  // macOS 的红黄绿按钮在左上角，给它们让出位置；其他系统的窗口按钮自己画在右边
  const nativeLights = isTauri && isMac;
  const ownControls = isTauri && !isMac;

  const importFiles = async (mode?: "managed" | "indexed") => importPaths(await pickFiles(), mode);
  const importFolder = async (mode?: "managed" | "indexed") => {
    const folder = await pickFolder();
    if (folder) await importPaths([folder], mode);
  };

  return (
    <header
      data-tauri-drag-region
      className="flex h-11 shrink-0 items-center gap-1 border-b border-line bg-side pr-2"
      style={{ paddingLeft: nativeLights ? 80 : 12 }}
    >
      <Logo />
      <span className="pointer-events-none ml-1 text-[13px] font-semibold">ShibaPack</span>
      <div data-tauri-drag-region className="h-full flex-1" />
      {!bare && (
        <>
          <Dropdown
            items={[
              { label: t("add.files"), icon: Plus, onSelect: () => void importFiles() },
              { label: t("add.folder"), icon: FolderInput, onSelect: () => void importFolder() },
              { label: t("add.filesManaged"), separated: true, onSelect: () => void importFiles("managed") },
              { label: t("add.filesIndexed"), onSelect: () => void importFiles("indexed") },
              { label: t("add.online"), icon: Globe, separated: true, onSelect: () => patch({ sourceDialog: { softwareId: null } }) },
            ]}
          >
            <Button variant="primary">
              <Plus size={15} strokeWidth={2} aria-hidden />
              {t("app.add")}
            </Button>
          </Dropdown>
          <Dropdown
            items={[
              { label: t("scan.folder"), icon: ScanSearch, onSelect: () => void importFolder("indexed") },
              { label: t("scan.tidy"), icon: FolderSync, onSelect: () => void tidyRoot() },
              { label: t("scan.updates"), icon: CloudDownload, separated: true, onSelect: () => void checkUpdates() },
            ]}
          >
            <Button variant="ghost">
              <RefreshCw size={15} strokeWidth={1.75} aria-hidden />
              {t("app.scan")}
            </Button>
          </Dropdown>
          <Button variant="ghost" onClick={() => patch({ settingsOpen: true })}>
            <Settings size={15} strokeWidth={1.75} aria-hidden />
            {t("app.settings")}
          </Button>
        </>
      )}
      {ownControls && <WindowControls />}
    </header>
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

/** 两栏之间的拖拽手柄。`edge` 说明它调整的是左边那一栏还是右边那一栏。 */
export function ResizeHandle({ edge, width, onResize }: { edge: "left" | "right"; width: number; onResize: (width: number) => void }) {
  const start = useRef<{ x: number; width: number } | null>(null);
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    start.current = { x: event.clientX, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const delta = event.clientX - start.current.x;
    onResize(start.current.width + (edge === "left" ? delta : -delta));
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
      <div className="truncate px-3 pt-1.5 text-[12px] text-fg-3">{text}</div>
    </div>
  );
}

export function StatusBar() {
  const t = useT();
  const library = useApp((state) => state.library);
  const status = useApp((state) => state.status);
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
  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-line bg-side px-3 text-[12px] text-fg-3">
      <span className="flex items-center gap-1.5">
        <span className={cx("size-1.5 shrink-0 rounded-full", busy ? "bg-accent" : "bg-success")} />
        <span className="max-w-[360px] truncate">{activity}</span>
      </span>
      <span className="tabular">{t("status.software", { n: library.software.length })}</span>
      <span className="tabular">{t("status.files", { n: files })}</span>
      <span className="tabular">{formatSize(size)}</span>
      <button
        type="button"
        title={t("storage.open")}
        onClick={() => void attempt(() => api.openRepoFolder())}
        className="ml-auto flex min-w-0 items-center gap-1.5 rounded px-1 hover:text-fg-2"
      >
        <FolderSearch size={13} strokeWidth={1.75} aria-hidden className="shrink-0" />
        <span className="truncate">{status?.repoRoot}</span>
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
  const toast = useApp((state) => state.toast);
  if (!toast) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-10 z-50 flex justify-center px-6">
      <div
        role="status"
        className={cx(
          "selectable pointer-events-auto max-w-[560px] animate-pop rounded-lg bg-panel px-3 py-2 text-[13px] shadow-pop",
          toast.kind === "error" ? "text-danger" : "text-fg",
        )}
      >
        {toast.text}
      </div>
    </div>
  );
}
