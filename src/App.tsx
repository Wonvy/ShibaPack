// 整体布局：顶栏 / 侧栏 + 中间栏 + 详情栏 / 底栏。

import { Download } from "lucide-react";
import { useEffect } from "react";
import { ImportProgress, ResizeHandle, StatusBar, ToastView, TopBar, UpdateCard } from "./components/chrome";
import { DialogHost } from "./components/dialogs";
import { PackageDetail } from "./components/PackageDetail";
import { SettingsDialog, SetupGuide } from "./components/panels";
import { Sidebar } from "./components/Sidebar";
import { SourceDialog } from "./components/SourcePanel";
import { onFileDrop } from "./lib/api";
import { useDrag } from "./lib/drag";
import { usePrefs } from "./lib/prefs";
import { useApp, useT } from "./lib/store";
import { scheduleUpdateChecks } from "./lib/updater";
import { InstalledDetail, InstalledView } from "./views/InstalledView";
import { LibraryView } from "./views/LibraryView";
import { StorageView } from "./views/StorageView";
import { TagsView } from "./views/TagsView";

export default function App() {
  const t = useT();
  const status = useApp((state) => state.status);
  const ready = useApp((state) => state.library !== null);
  const route = useApp((state) => state.route);
  const dropping = useApp((state) => state.dropping);
  const sidebarWidth = usePrefs((state) => state.sidebarWidth);
  const detailWidth = usePrefs((state) => state.detailWidth);
  const setPrefs = usePrefs((state) => state.set);

  useEffect(() => {
    const app = useApp.getState();
    void app.init().then(async () => {
      // 开发用的自检模式：只有带着环境变量启动桌面窗口时才会走到
      if (useApp.getState().status?.selftest) (await import("./lib/selftest")).runSelftest();
      // 自检时不查更新，免得两边互相干扰
      else scheduleUpdateChecks();
    });
    // 把文件拖进窗口就导入
    const off = onFileDrop(
      (paths) => void useApp.getState().importPaths(paths),
      (over) => useApp.getState().patch({ dropping: over }),
    );
    return () => void off.then((stop) => stop());
  }, []);

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

  // 标签页和存储页不需要详情栏，中间栏占满
  const withDetail = route === "library" || route === "installed";

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <ResizeHandle edge="left" width={sidebarWidth} onResize={(width) => setPrefs({ sidebarWidth: width })} />
        <main className="relative flex min-w-0 flex-1 flex-col">
          <ImportProgress />
          {route === "library" && <LibraryView />}
          {route === "installed" && <InstalledView />}
          {route === "tags" && <TagsView />}
          {route === "storage" && <StorageView />}
          {dropping && (
            <div className="pointer-events-none absolute inset-2 z-20 flex animate-fade flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed border-accent bg-accent-soft text-accent backdrop-blur-sm">
              <Download size={28} strokeWidth={1.5} aria-hidden />
              <span className="text-[13px] font-medium">{t("import.drop")}</span>
            </div>
          )}
        </main>
        {withDetail && (
          <>
            <ResizeHandle edge="right" width={detailWidth} onResize={(width) => setPrefs({ detailWidth: width })} />
            <aside className="min-h-0 shrink-0" style={{ width: detailWidth }}>
              {route === "library" ? <PackageDetail /> : <InstalledDetail />}
            </aside>
          </>
        )}
      </div>
      <StatusBar />
      <SettingsDialog />
      <SourceDialog />
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
