// ShibaPack 自己的在线更新：检查 → 下载 → 验证签名 → 安装 → 重启。
//
// 更新包放在 GitHub Releases 里，地址和验签用的公钥写在 src-tauri/tauri.conf.json 的 plugins.updater 里。
// macOS 和 Linux 上可以在应用运行时把新版本装好，重启后生效；
// Windows 的安装程序必须先关掉应用，所以下载完成后要等用户点一下才安装。

import { hostPlatform, isTauri } from "./api";
import { type AppUpdate, useApp } from "./store";

type DownloadEvent =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** 更新插件给出的“一个可用的更新”里，这里用得到的部分。 */
interface Pending {
  version: string;
  download: (onEvent?: (event: DownloadEvent) => void) => Promise<void>;
  install: () => Promise<void>;
}

const installsInPlace = hostPlatform !== "windows";
let pending: Pending | null = null;
let busy = false;

const set = (patch: Partial<AppUpdate>) => useApp.setState({ appUpdate: { ...useApp.getState().appUpdate, ...patch } });
const autoUpdate = () => useApp.getState().library?.settings.autoUpdate ?? true;

async function lookUp(): Promise<Pending | null> {
  if (!isTauri) return previewUpdate();
  const { check } = await import("@tauri-apps/plugin-updater");
  return check();
}

/**
 * 检查有没有新版本。`manual` 是用户在设置里点的：结果（包括“已是最新”和出错）都要让他看到；
 * 启动时自动检查的，没有新版本或网络不通就悄悄结束。
 */
export async function checkForUpdate(manual: boolean) {
  const phase = useApp.getState().appUpdate.phase;
  // 已经下载好或装好了，就不用再查
  if (busy || phase === "downloading" || phase === "ready" || phase === "installed") return;
  busy = true;
  try {
    if (manual) set({ phase: "checking", error: "", dismissed: false });
    pending = await lookUp();
    if (!pending) {
      set({ phase: manual ? "upToDate" : "idle" });
      return;
    }
    set({ phase: "available", version: pending.version, dismissed: false });
    if (autoUpdate()) await download();
  } catch (error) {
    set(manual ? { phase: "error", error: String(error) } : { phase: "idle" });
  } finally {
    busy = false;
  }
}

/** 下载新版本；能在运行时安装的系统上顺便装好。 */
async function download() {
  if (!pending) return;
  set({ phase: "downloading", progress: null });
  let total = 0;
  let received = 0;
  await pending.download((event) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    if (event.event === "Progress") {
      received += event.data.chunkLength;
      set({ progress: total > 0 ? Math.min(1, received / total) : null });
    }
  });
  if (installsInPlace) {
    await pending.install();
    set({ phase: "installed" });
  } else {
    set({ phase: "ready" });
  }
}

/** 用户点了“下载并安装”（自动更新关着时才会走到）。 */
export async function startUpdate() {
  if (busy) return;
  busy = true;
  try {
    await download();
  } catch (error) {
    set({ phase: "error", error: String(error) });
  } finally {
    busy = false;
  }
}

/** 用户点了“安装并重启”或“立即重启”。 */
export async function finishUpdate() {
  try {
    // Windows：这一步会关掉应用，安装程序装完后再把它打开
    if (useApp.getState().appUpdate.phase === "ready") await pending?.install();
    if (isTauri) {
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } else {
      location.reload();
    }
  } catch (error) {
    set({ phase: "error", error: String(error) });
  }
}

export function dismissUpdate() {
  set({ dismissed: true });
}

/** 启动后过几秒查一次，之后每六小时查一次。只在开着“自动更新”时查。 */
export function scheduleUpdateChecks() {
  const run = () => autoUpdate() && void checkForUpdate(false);
  setTimeout(run, 4000);
  setInterval(run, 6 * 3600 * 1000);
}

/** 浏览器预览里没有更新插件。地址带上 ?update 时假装有一个新版本，用来看界面。 */
function previewUpdate(): Pending | null {
  if (!location.search.includes("update")) return null;
  return {
    version: "0.2.0",
    download: async (onEvent) => {
      onEvent?.({ event: "Started", data: { contentLength: 100 } });
      for (let step = 0; step < 10; step++) {
        await new Promise((resolve) => setTimeout(resolve, 250));
        onEvent?.({ event: "Progress", data: { chunkLength: 10 } });
      }
      onEvent?.({ event: "Finished" });
    },
    install: async () => {},
  };
}
