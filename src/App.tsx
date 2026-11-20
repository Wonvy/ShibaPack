// 整体布局：顶栏 / 侧栏 + 中间栏 + 详情栏 / 底栏。

import { Download } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { ClassifyDialog } from "./components/ClassifyDialog";
import { UninstallDialog } from "./components/UninstallDialog";
import { AddMenuButton, DetailToggle, ResizeHandle, StatusBar, ToastView, TopBar, UpdateCard } from "./components/chrome";
import { DialogHost } from "./components/dialogs";
import { ImportDialog } from "./components/ImportDialog";
import { PackageDetail } from "./components/PackageDetail";
import { SettingsDialog, SetupGuide } from "./components/panels";
import { Sidebar } from "./components/Sidebar";
import { SourceDialog } from "./components/SourcePanel";
import { UpdateDialog } from "./components/UpdateDialog";
import { onFileDrop, windowControls } from "./lib/api";
import { useDrag } from "./lib/drag";
import { dropOnCategory, fileDropTargetAt } from "./lib/filedrop";
import { SIDEBAR, usePrefs } from "./lib/prefs";
import { useStartup } from "./lib/startup";
import { type Route, useApp, useT } from "./lib/store";
import { scheduleUpdateChecks } from "./lib/updater";
import { InstalledDetail, InstalledView } from "./views/InstalledView";
import { LibraryView } from "./views/LibraryView";
import { StartupDetail, StartupView } from "./views/StartupView";
import { CleanupDetail, CleanupView } from "./views/CleanupView";
import { CleanupDialogs } from "./components/CleanupDialogs";
import { useCleanup } from "./lib/cleanup";

export default function App() {
  const t = useT();
  const status = useApp((state) => state.status);
  const ready = useApp((state) => state.library !== null);
  const route = useApp((state) => state.route);
  const dropping = useApp((state) => state.dropping);
  const manualSidebarWidth = usePrefs((state) => state.sidebarWidth);
  const [autoSidebarWidth, setAutoSidebarWidth] = useState(SIDEBAR.min);
  const sidebarWidth = manualSidebarWidth ?? autoSidebarWidth;
  const detailWidth = usePrefs((state) => state.detailWidth);
  const detailOpen = usePrefs((state) => state.detailOpen);
  const setPrefs = usePrefs((state) => state.set);
  const startupSelected = useStartup((state) => state.selectedId !== null);
  const cleanupSelected = useCleanup((state) => state.selectedId !== null);
  const softwareSelected = useApp((state) => (state.route === "installed" ? state.selectedInstalledId : state.selectedId) !== null);
  const hasSelection = route === "startup" ? startupSelected : route === "cleanup" ? cleanupSelected : softwareSelected;

  // 存储页不需要详情栏；详情栏收起时中间栏占满。
  // 窗口太窄、中间栏放不下时，详情栏不再占一栏，选中软件时浮在列表右侧
  const withDetail = (route === "library" || route === "installed" || route === "startup" || route === "cleanup") && detailOpen;
  const dockMinWidth = sidebarWidth + detailWidth + MAIN_MIN;
  const { width: windowWidth, toggle, resizing } = useWindowLayout(detailWidth, dockMinWidth);
  const cramped = windowWidth < dockMinWidth;
  const docked = withDetail && !cramped;
  const floating = withDetail && cramped && hasSelection;
  const floatingDetailWidth = Math.min(detailWidth, Math.max(0, windowWidth - sidebarWidth - 48));

  useEffect(() => {
    const app = useApp.getState();
    void app.init().then(async () => {
      // 开发用的自检模式：只有带着环境变量启动桌面窗口时才会走到
      if (useApp.getState().status?.selftest) (await import("./lib/selftest")).runSelftest();
      // 自检时不查更新，免得两边互相干扰
      else scheduleUpdateChecks();
    });
    // 从系统拖文件进窗口：
    // - 落在侧栏的某个分类上：直接导入进这个分类（带着文件夹时先打开导入窗口，问文件夹是整个导入还是导入里面的安装包）
    // - 落在别处：打开导入窗口，选分类和存储方式，确认后才导入
    // 不按落点归到某个软件下：拖的时候很容易停在别的软件或详情栏上，结果成了那个软件的版本
    const off = onFileDrop(
      (paths, point) => {
        const app = useApp.getState();
        const target = app.fileDropTarget ?? fileDropTargetAt(point);
        app.patch({ fileDropTarget: null });
        if (!app.library) return;
        if (target) void dropOnCategory(paths, target === "uncategorized" ? null : target.slice("category:".length));
        else app.patch({ importDialog: { paths } });
      },
      (over, point) => {
        const app = useApp.getState();
        const target = over && point ? fileDropTargetAt(point) : null;
        // 停在侧栏的分类上时，那一行变色，中间栏不再显示大的“松开导入”
        const dropping = over && target === null;
        if (app.dropping !== dropping || app.fileDropTarget !== target) app.patch({ dropping, fileDropTarget: target });
      },
    );
    return () => void off.then((stop) => stop());
  }, []);

  // 窗口回到前台时：索引被别处改过（另一台电脑、手动编辑）就换上新的；
  // 再重新列一遍已安装的软件，在外面装完、卸完回来，本机版本马上对得上。
  // 已安装的软件至少隔 15 秒才扫一次，免得来回切窗口时反复扫描
  useEffect(() => {
    const rescan = () => {
      if (document.visibilityState === "hidden") return;
      const app = useApp.getState();
      void app.refreshLibrary();
      // 正看着启动项时也重新列一遍：可能刚在系统设置里改过
      const startup = useStartup.getState();
      if (app.route === "startup" && startup.items !== null && !startup.loading && Date.now() - startup.scannedAt > 15_000) void startup.load();
      if (app.installed === null || app.installedLoading || Date.now() - app.installedScannedAt < 15_000) return;
      void app.loadInstalled();
    };
    window.addEventListener("focus", rescan);
    document.addEventListener("visibilitychange", rescan);
    return () => {
      window.removeEventListener("focus", rescan);
      document.removeEventListener("visibilitychange", rescan);
    };
  }, []);

  // 浮动的详情栏按 Esc 关闭。输入框、对话框和菜单里的 Esc 留给它们自己
  useEffect(() => {
    if (!floating) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      const app = useApp.getState();
      if (app.route === "installed") app.selectInstalled(null);
      else if (app.route === "startup") useStartup.getState().select(null);
      else if (app.route === "cleanup") useCleanup.getState().select(null);
      else app.select(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [floating]);

  // 状态还没回来：只铺底色，不闪引导页
  if (!status) return <div className="h-full bg-bg" />;

  if (!status.configured || !ready) {
    return (
      <div className="flex h-full flex-col">
        <TopBar />
        <SetupGuide />
        <ToastView />
      </div>
    );
  }

  const detailToggle = <DetailToggle open={detailOpen} onClick={toggle} disabled={resizing} />;

  return (
    <div className="flex h-full flex-col">
      <TopBar listBounds={{ left: sidebarWidth, right: docked ? detailWidth : 0, windowWidth }} />
      <div className="relative flex min-h-0 flex-1">
        <Sidebar onAutoWidthChange={setAutoSidebarWidth} />
        <ResizeHandle
          edge="left"
          width={sidebarWidth}
          onResize={(width) => setPrefs({ sidebarWidth: width })}
          onReset={() => setPrefs({ sidebarWidth: null })}
        />
        <main className="relative flex min-w-0 flex-1 flex-col">
          {route === "library" && <LibraryView detailToggle={detailToggle} />}
          {route === "installed" && <InstalledView detailToggle={detailToggle} />}
          {route === "startup" && <StartupView detailToggle={detailToggle} />}
          {route === "cleanup" && <CleanupView detailToggle={detailToggle} />}
          {dropping && (
            <div className="pointer-events-none absolute inset-2 z-20 flex animate-fade flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent bg-accent-soft text-accent backdrop-blur-sm">
              <Download size={28} strokeWidth={1.5} aria-hidden />
              <span className="text-[13px] font-medium">{t("import.drop")}</span>
            </div>
          )}
          {/* 清理页底部有自己的清理按钮，不放添加按钮 */}
          {route !== "cleanup" && <AddMenuButton coveredWidth={floating ? floatingDetailWidth : 0} />}
          {/* 状态栏只占中间一栏，左边的侧栏和右边的详情栏一直到窗口底部 */}
          <StatusBar />
        </main>
        {docked && (
          <>
            <ResizeHandle edge="right" width={detailWidth} onResize={(width) => setPrefs({ detailWidth: width })} />
            <aside className="min-h-0 shrink-0 motion-safe:animate-fade" style={{ width: detailWidth }}>
              <DetailFor route={route} />
            </aside>
          </>
        )}
        {floating && (
          // 让出中间栏的工具栏和状态栏，点列表空白处或按 Esc 关闭
          <aside
            className="absolute right-0 bottom-(--bottom-bar-height) z-30 min-h-0 animate-fade border-l border-line bg-bg shadow-pop"
            style={{ width: floatingDetailWidth, top: route !== "installed" && windowWidth - sidebarWidth < 440 ? 68 : 44 }}
          >
            <DetailFor route={route} />
          </aside>
        )}
      </div>
      <SettingsDialog />
      <SourceDialog />
      <ImportDialog />
      <ClassifyDialog />
      <UninstallDialog />
      <CleanupDialogs />
      <DialogHost />
      <ToastView />
      <UpdateCard />
      <UpdateDialog />
      <DragGhost />
    </div>
  );
}

/** 拖拽时跟着指针走的小标签，说明正在拖什么、会落到哪里。 */
function DragGhost() {
  const t = useT();
  const payload = useDrag((state) => state.payload);
  const target = useDrag((state) => state.target);
  const x = useDrag((state) => state.x);
  const y = useDrag((state) => state.y);
  const categories = useApp((state) => state.library?.categories);
  if (!payload) return null;
  const targetName =
    target === null ? null : target.category === null ? t("nav.uncategorized") : categories?.find((c) => c.id === target.category)?.name;
  const label = payload.kind === "software" && targetName ? t("drag.toCategory", { name: targetName }) : payload.label;
  return (
    <div
      className="pointer-events-none fixed z-50 max-w-[240px] truncate rounded-md bg-panel px-2 py-1 text-[12px] shadow-pop"
      style={{ left: x + 14, top: y + 16 }}
    >
      {label}
    </div>
  );
}

/** 中间栏至少要有这么宽，放不下时详情栏改成浮动的。 */
const MAIN_MIN = 380;

type PendingDetailResize = { width: number; finish: () => void; timer?: number };

function useWindowLayout(detailWidth: number, dockMinWidth: number) {
  const [width, setWidth] = useState(() => window.innerWidth);
  const [resizing, setResizing] = useState(false);
  const pending = useRef<PendingDetailResize | null>(null);

  useEffect(() => {
    const onResize = () => {
      const change = pending.current;
      if (change && change.width !== window.innerWidth) change.finish();
      else setWidth(window.innerWidth);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      window.clearTimeout(pending.current?.timer);
      pending.current = null;
    };
  }, []);

  const toggle = () => {
    if (pending.current) return;
    const prefs = usePrefs.getState();
    const open = !prefs.detailOpen;
    const request = windowControls.resizeForDetail(open, detailWidth, dockMinWidth);
    if (!request) {
      prefs.set({ detailOpen: open });
      return;
    }

    const change: PendingDetailResize = {
      width: window.innerWidth,
      finish: () => {
        if (pending.current !== change) return;
        window.clearTimeout(change.timer);
        pending.current = null;
        // 在同一帧提交新窗口宽度和详情栏状态，避免网格先换列又换回来。
        flushSync(() => {
          setWidth(window.innerWidth);
          prefs.set({ detailOpen: open });
          setResizing(false);
        });
      },
    };
    pending.current = change;
    setResizing(true);
    void request.then((result) => {
      if (pending.current !== change) return;
      if (!result || window.innerWidth !== change.width) change.finish();
      else change.timer = window.setTimeout(change.finish, 180);
    });
  };

  return { width, toggle, resizing };
}

/** 详情栏里放哪一个：跟着中间栏的页面走。 */
function DetailFor({ route }: { route: Route }) {
  if (route === "library") return <PackageDetail />;
  if (route === "installed") return <InstalledDetail />;
  if (route === "cleanup") return <CleanupDetail />;
  return <StartupDetail />;
}
