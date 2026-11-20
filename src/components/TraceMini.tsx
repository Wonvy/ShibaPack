// 安装监控的浮动小窗：单独的一个窗口，总在最前面，放在屏幕右上角。
// 装的时候不用切回 ShibaPack 也能看到装到哪了、点“完成”。监控结束（主窗口里完成或放弃）时主窗口把它关掉。

import { Radar, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useApp, useT } from "../lib/store";
import { applyTheme } from "../lib/theme";
import type { TraceStatus } from "../lib/types";
import { LiveView } from "./TraceDialog";
import { Button } from "./ui";

export function TraceMini() {
  const t = useT();
  const [status, setStatus] = useState<TraceStatus | null>(null);

  // 用和主窗口一样的语言、主题
  useEffect(() => {
    void api.getLibrary().then(
      (library) => {
        useApp.setState({ library });
        applyTheme(library.settings.theme, library.settings.accent);
      },
      () => {},
    );
  }, []);

  useEffect(() => {
    let alive = true;
    const poll = () =>
      void api.traceStatus().then((next) => {
        if (!alive) return;
        // 监控已经结束了（主窗口里完成或放弃）
        if (!next) void closeSelf();
        else setStatus(next);
      });
    poll();
    const timer = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  const elapsed = Math.floor((status?.elapsedMs ?? 0) / 1000);
  const clock = `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
  const exited = status?.installerRunning === false;

  return (
    <div className="flex h-screen flex-col overflow-hidden rounded-xl border border-line bg-panel p-3 text-fg select-none">
      <div data-tauri-drag-region className="flex items-center gap-2">
        <Radar size={14} strokeWidth={1.75} aria-hidden className="pointer-events-none shrink-0 text-accent" />
        <span data-tauri-drag-region className="min-w-0 flex-1 truncate text-[12px] font-semibold">
          {status ? `${status.softwareName} ${status.version}`.trim() : t("trace.watching")}
        </span>
        <span className="tabular pointer-events-none shrink-0 text-[12px] text-fg-3">{clock}</span>
        <button
          type="button"
          title={t("trace.miniHide")}
          aria-label={t("trace.miniHide")}
          onClick={() => void closeSelf()}
          className="flex size-5 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg"
        >
          <X size={12} strokeWidth={2} aria-hidden />
        </button>
      </div>
      <div className="mt-2 min-h-0 flex-1">
        {status?.live ? <LiveView live={status.live} compact /> : <p className="m-0 text-[12px] text-fg-3">{t("trace.installerUnknown")}</p>}
      </div>
      <div className="mt-2 flex items-center gap-2">
        <span className={exited || status?.rebootRequired ? "min-w-0 flex-1 truncate text-[12px] text-success" : "min-w-0 flex-1 truncate text-[12px] text-fg-3"}>
          {status?.rebootRequired ? t("trace.miniReboot") : exited ? t("trace.miniExited") : t("trace.miniRunning")}
        </span>
        <Button size="sm" variant="ghost" onClick={() => void api.traceMiniAction("open")}>
          {t("trace.miniOpen")}
        </Button>
        <Button size="sm" variant="primary" onClick={() => void api.traceMiniAction("finish")}>
          {t("trace.finish")}
        </Button>
      </div>
    </div>
  );
}

async function closeSelf() {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  } catch {
    // 浏览器预览里没有窗口可关
  }
}
