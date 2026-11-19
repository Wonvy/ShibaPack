import { spawn } from "node:child_process";
import { watch } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// 需要监听自动重启的源码目录/文件（相对项目根）
const WATCH_PATHS = [
  "src",
  "index.html",
  "vite.config.ts",
  path.join("src-tauri", "src"),
  path.join("src-tauri", "Cargo.toml"),
  path.join("src-tauri", "tauri.conf.json"),
];

// 忽略频繁变动或无关的文件，避免误触发重启
const IGNORE = [/\.tmp$/, /\.swp$/, /~$/, /\.lock$/, [/node_modules/, /dist/, /target/, /\.git/]];

const DEBOUNCE_MS = 600;
// 应用启动后的稳定期：这段时间内忽略文件事件，避免 Vite/cargo 初始化写入触发误重启
const STARTUP_GRACE_MS = 8000;
let child = null;
let timer = null;
let stopping = false;
let runId = 0;
let startedAt = 0;

function log(...args) {
  console.log("[watch]", ...args);
}

function startApp() {
  const id = ++runId;
  startedAt = Date.now();
  log("启动应用…");
  child = spawn("npm", ["run", "app"], {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
    detached: process.platform !== "win32",
  });
  child.on("error", (err) => log("启动失败:", err.message));
  child.on("exit", (code, signal) => {
    if (id !== runId) return; // 已被重启/停止，忽略旧进程退出事件
    child = null;
    if (!stopping) log(`应用退出 (code=${code}, signal=${signal})`);
  });
}

function killApp() {
  if (!child || !child.pid) return;
  const pid = child.pid;
  const current = child;
  child = null;
  try {
    if (process.platform === "win32") {
      // 结束整棵进程树（npm → tauri → cargo → app）
      spawn("taskkill", ["/PID", String(pid), "/T", "/F"], { stdio: "ignore" });
    } else {
      process.kill(-pid, "SIGTERM"); // detached 进程组
    }
  } catch (err) {
    log("结束进程失败:", err.message);
  }
  // 给端口释放留出时间
  return new Promise((resolve) => setTimeout(resolve, 800));
}

async function restart(reason) {
  if (stopping) return;
  log(`检测到改动 (${reason})，重启…`);
  await killApp();
  startApp();
}

function shouldIgnore(rel) {
  const normalized = String(rel).replace(/\\/g, "/");
  return IGNORE.some((rule) =>
    Array.isArray(rule) ? rule.some((r) => r.test(normalized)) : rule.test(normalized),
  );
}

for (const rel of WATCH_PATHS) {
  const target = path.join(root, rel);
  try {
    watch(target, { recursive: true }, (_event, filename) => {
      // Windows 上递归监听常产生 filename 为空的虚假事件，直接忽略
      if (!filename) return;
      // 启动稳定期内忽略事件，避免 Vite/cargo 初始化写入触发误重启
      if (Date.now() - startedAt < STARTUP_GRACE_MS) return;
      const file = String(filename);
      if (shouldIgnore(path.join(rel, file))) return;
      clearTimeout(timer);
      timer = setTimeout(() => void restart(rel), DEBOUNCE_MS);
    });
  } catch (err) {
    log(`监听 ${rel} 失败:`, err.message);
  }
}

function shutdown() {
  stopping = true;
  clearTimeout(timer);
  log("停止监听并关闭应用");
  void killApp().finally(() => process.exit(0));
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

log(`监听源码变化，自动重启应用 (防抖 ${DEBOUNCE_MS}ms)`);
log("监听路径:", WATCH_PATHS.join(", "));
startApp();
