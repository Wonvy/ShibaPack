// 整体布局：顶栏 / 侧栏 + 中间栏 + 详情栏 / 底栏。

import { Download } from "lucide-react";
import { useEffect, useState } from "react";
import { ClassifyDialog } from "./components/ClassifyDialog";
import { UninstallDialog } from "./components/UninstallDialog";
import { ImportProgress, ResizeHandle, StatusBar, ToastView, TopBar, UpdateCard } from "./components/chrome";
import { DialogHost } from "./components/dialogs";
import { PackageDetail } from "./components/PackageDetail";
import { SettingsDialog, SetupGuide } from "./components/panels";
import { Sidebar } from "./components/Sidebar";
import { SourceDialog } from "./components/SourcePanel";
import { type DropPoint, onFileDrop } from "./lib/api";
import { useDrag } from "./lib/drag";
import { RAIL_WIDTH, SIDEBAR, usePrefs } from "./lib/prefs";
import { useApp, useT } from "./lib/store";
import { scheduleUpdateChecks } from "./lib/updater";
import { InstalledDetail, InstalledView } from "./views/InstalledView";
import { LibraryView } from "./views/LibraryView";

export default function App() {
  const t = useT();
  const status = useApp((state) => state.status);
  const ready = useApp((state) => state.library !== null);
  const route = useApp((state) => state.route);
  const dropping = useApp((state) => state.dropping);
  const dropName = useApp((state) => state.library?.software.find((s) => s.id === state.dropSoftwareId)?.name);
  const sidebarCollapsed = usePrefs((state) => state.sidebarCollapsed);
  // 侧栏收起时只占一列图标的宽度
  const sidebarWidth = usePrefs((state) => (state.sidebarCollapsed ? RAIL_WIDTH : state.sidebarWidth));
  const detailWidth = usePrefs((state) => state.detailWidth);
  const detailOpen = usePrefs((state) => state.detailOpen);
  const setPrefs = usePrefs((state) => state.set);
  const windowWidth = useWindowWidth();
  const hasSelection = useApp((state) => (state.route === "installed" ? state.selectedInstalledId : state.selectedId) !== null);

  // 存储页不需要详情栏；详情栏收起时中间栏占满。
  // 窗口太窄、中间栏放不下时，详情栏不再占一栏，选中软件时浮在列表右侧
  const withDetail = (route === "library" || route === "installed") && detailOpen;
  const cramped = windowWidth - sidebarWidth - detailWidth < MAIN_MIN;
  const docked = withDetail && !cramped;
  const floating = withDetail && cramped && hasSelection;

  useEffect(() => {
    const app = useApp.getState();
    void app.init().then(async () => {
      // 开发用的自检模式：只有带着环境变量启动桌面窗口时才会走到
      if (useApp.getState().status?.selftest) (await import("./lib/selftest")).runSelftest();
      // 自检时不查更新，免得两边互相干扰
      else scheduleUpdateChecks();
    });
    // 把文件拖进窗口就导入；落在某个软件（列表里的一行或详情栏）上时，作为它的新版本导入
    const softwareAt = (point: DropPoint | null) =>
      point ? (document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>("[data-software-id]")?.dataset.softwareId ?? null) : null;
    const off = onFileDrop(
      (paths, point) => {
        const app = useApp.getState();
        const target = app.library?.software.find((s) => s.id === softwareAt(point));
        void app.importPaths(paths, undefined, target ? { name: target.name } : undefined);
      },
      (over, point) => {
        const app = useApp.getState();
        const id = over ? softwareAt(point) : null;
        if (app.dropping !== over || app.dropSoftwareId !== id) app.patch({ dropping: over, dropSoftwareId: id });
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
      if (event.key !== "Escape" || event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
      if (document.querySelector('[role="dialog"],[role="menu"]')) return;
      const app = useApp.getState();
      if (app.route === "installed") app.patch({ selectedInstalledId: null });
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
        <TopBar bare />
        <SetupGuide />
        <ToastView />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="relative flex min-h-0 flex-1">
        <Sidebar />
        {!sidebarCollapsed && (
          <ResizeHandle
            edge="left"
            width={sidebarWidth}
            onResize={(width) => setPrefs({ sidebarWidth: width })}
            // 拖得比最窄还窄一截，就收起成一列图标
            collapseBelow={SIDEBAR.min - 48}
            onCollapse={() => setPrefs({ sidebarCollapsed: true })}
          />
        )}
        <main className="relative flex min-w-0 flex-1 flex-col">
          <ImportProgress />
          {route === "library" && <LibraryView />}
          {route === "installed" && <InstalledView />}
          {dropping && !dropName && (
            <div className="pointer-events-none absolute inset-2 z-20 flex animate-fade flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent bg-accent-soft text-accent backdrop-blur-sm">
              <Download size={28} strokeWidth={1.5} aria-hidden />
              <span className="text-[13px] font-medium">{t("import.drop")}</span>
              <span className="text-[12px] opacity-80">{t("drop.onSoftwareHint")}</span>
            </div>
          )}
          {dropping && dropName && (
            // 停在某个软件上：那一行自己高亮，这里只在顶上说明松手后会怎样
            <div className="pointer-events-none absolute inset-x-0 top-12 z-20 flex justify-center px-4">
              <div className="flex max-w-full animate-pop items-center gap-1.5 truncate rounded-full bg-accent-strong px-3 py-1 text-[12px] font-medium text-white shadow-pop">
                <Download size={13} strokeWidth={2} aria-hidden className="shrink-0" />
                <span className="truncate">{t("drop.toSoftware", { name: dropName })}</span>
              </div>
            </div>
          )}
          {/* 状态栏只占中间一栏，左边的侧栏和右边的详情栏一直到窗口底部 */}
          <StatusBar />
        </main>
        {docked && (
          <>
            <ResizeHandle edge="right" width={detailWidth} onResize={(width) => setPrefs({ detailWidth: width })} />
            <aside className="min-h-0 shrink-0" style={{ width: detailWidth }}>
              {route === "library" ? <PackageDetail /> : <InstalledDetail />}
            </aside>
          </>
        )}
        {floating && (
          // 让出中间栏的工具栏（44px）和底下的状态栏（28px），点列表空白处或按 Esc 关闭
          <aside
            className="absolute top-11 right-0 bottom-7 z-30 min-h-0 animate-fade border-l border-line bg-bg shadow-pop"
            style={{ width: Math.min(detailWidth, windowWidth - sidebarWidth - 48) }}
          >
            {route === "library" ? <PackageDetail /> : <InstalledDetail />}
          </aside>
        )}
      </div>
      <SettingsDialog />
      <SourceDialog />
      <ClassifyDialog />
      <UninstallDialog />
      <DialogHost />
      <ToastView />
      <UpdateCard />
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

function useWindowWidth(): number {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return width;
}
