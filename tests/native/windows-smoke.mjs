import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readdir, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

assert.equal(process.platform, "win32");
const run = promisify(execFile);
const runHidden = (command, args) => run(command, args, { windowsHide: true, timeout: 15000 });
const executable = resolve(process.argv[2] ?? "target/release/shibapack.exe");
const fixtureSource = fileURLToPath(new URL("./uninstaller.rs", import.meta.url));
const windowScript = fileURLToPath(new URL("./windows-window.ps1", import.meta.url));
const nativeWindow = (action, args) => runHidden("pwsh.exe", ["-NoProfile", "-File", windowScript, "-Action", action, ...args]);
const temp = await mkdtemp(join(tmpdir(), "shibapack-native-smoke-"));
const repo = join(temp, "repo");
const profile = join(temp, "webview");
const config = join(temp, "config.json");
const caption = "ShibaPack-Smoke-" + randomUUID();
const registryKey = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\" + caption;
let session;
let registered = false;

async function until(check, message, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value;
    await delay(100);
  }
  throw new Error(message);
}

function connect(url) {
  const socket = new WebSocket(url);
  const ready = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    pending.get(message.id)?.(message);
  });
  return {
    close: () => socket.close(),
    async evaluate(callback, args) {
      await ready;
      const request = ++id;
      const result = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          pending.delete(request);
          reject(new Error("Desktop command timed out"));
        }, 30000);
        pending.set(request, (message) => {
          clearTimeout(timer);
          pending.delete(request);
          resolve(message);
        });
        const expression = "(" + callback.toString() + ")(" + JSON.stringify(args) + ")";
        socket.send(JSON.stringify({ id: request, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
      });
      assert.ok(!result.error, JSON.stringify(result.error));
      if (result.result.exceptionDetails) throw new Error(result.result.result.description ?? JSON.stringify(result.result.result.value));
      return result.result.result.value;
    },
  };
}

async function start() {
  const reservation = createServer();
  await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
  const { port } = reservation.address();
  await new Promise((resolve) => reservation.close(resolve));
  const child = spawn(executable, {
    cwd: temp, windowsHide: true, stdio: ["ignore", "ignore", "pipe"],
    env: {
      ...process.env,
      SHIBAPACK_CONFIG: config, SHIBAPACK_CACHE: join(temp, "cache"),
      WEBVIEW2_USER_DATA_FOLDER: profile, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=" + port,
    },
  });
  session = { child };
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += chunk; });
  const target = await until(async () => {
    if (child.exitCode !== null) throw new Error("Test app exited: " + stderr);
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(500) })).json();
      return targets.find((entry) => entry.type === "page" && entry.url.includes("tauri.localhost"));
    } catch { return undefined; }
  }, "No local test WebView");
  assert.ok((await readdir(profile)).length > 0);
  session.webview = connect(target.webSocketDebuggerUrl);
  await until(() => session.webview.evaluate(() => Boolean(window.__TAURI_INTERNALS__)), "Desktop bridge did not load");
  assert.equal((await invoke("get_status")).repoRoot, repo);
  await session.webview.evaluate(() => { location.hash = "/library"; });
  await until(() => session.webview.evaluate(() => Boolean(document.querySelector("[data-detail-toggle]"))), "Library did not render");
}

const invoke = (command, args = {}) => session.webview.evaluate(({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const button = (label, click = false) => session.webview.evaluate(({ label, click }) => {
  const element = Array.from(document.querySelectorAll("button[aria-label]"))
    .find((button) => button.getAttribute("aria-label") === label && !button.disabled && button.getClientRects().length);
  if (click && element) setTimeout(() => element.click(), 50);
  return Boolean(element);
}, { label, click });
const geometry = () => session.webview.evaluate(() => ({ width: innerWidth, height: innerHeight, x: screenX, y: screenY }));
const maximized = () => invoke("plugin:window|is_maximized", { label: "main" });
const readWindowState = async () => JSON.parse(await readFile(join(temp, "config.window-state.json"), "utf8")).main;
const sameGeometry = (a, b) => Object.keys(a).every((key) => Math.abs(a[key] - b[key]) <= 1);

async function close() {
  await until(() => button("关闭", true), "Close button was missing");
  await until(() => session.child.exitCode !== null, "Test app did not close");
  assert.equal(session.child.exitCode, 0);
  session.webview.close();
  session = undefined;
  return readWindowState();
}

async function testUninstaller() {
  const folder = join(temp, "Uninstaller Fixture");
  await mkdir(folder);
  const fixture = join(folder, "fixture.exe");
  await runHidden("rustc.exe", [fixtureSource, "-o", fixture]);
  registered = true;
  for (const [name, value] of Object.entries({
    DisplayName: caption, DisplayVersion: "1.0", InstallLocation: folder,
    UninstallString: `${fixture} ${caption}`,
  })) {
    await runHidden("reg.exe", ["add", registryKey, "/v", name, "/t", "REG_SZ", "/d", value, "/f"]);
  }
  const { apps } = await invoke("list_installed");
  const entry = apps.find((app) => app.name === caption);
  assert.ok(entry?.uninstallable, "Fixture registration not found");
  await invoke("uninstall_installed", { id: entry.id });
  await until(async () => (await nativeWindow("visible", ["-Caption", caption])).stdout.trim() === "visible", "Uninstaller window did not appear");
  assert.equal(await invoke("uninstall_status", { id: entry.id }), true);
  await nativeWindow("close", ["-Caption", caption]);
  await runHidden("reg.exe", ["delete", registryKey, "/f"]);
  registered = false;
  assert.equal(await invoke("uninstall_status", { id: entry.id }), false);
  assert.ok(!(await invoke("list_installed")).apps.some((app) => app.id === entry.id));
}

try {
  await mkdir(repo);
  await writeFile(config, JSON.stringify({ repoRoot: repo }));
  await writeFile(join(repo, "software.json"), JSON.stringify({
    format: 1, settings: { language: "zh-CN", autoUpdate: false, fetchSummaries: false, offlineCatalog: false },
  }));
  await start();
  await until(() => button("收起侧栏", true), "Sidebar control was missing");
  await until(() => button("隐藏详情栏", true), "Details control was missing");
  await until(() => button("显示详情栏"), "Details did not collapse");
  const before = await geometry();
  await nativeWindow("resize", ["-AppPid", String(session.child.pid), "-Executable", executable]);
  await until(async () => (await geometry()).height !== before.height, "Native resize did not reach the WebView");
  const restored = await geometry();
  assert.ok((await close()).width > 0);

  await start();
  await until(async () => sameGeometry(await geometry(), restored), "Window size or position was not restored");
  assert.ok(await button("展开侧栏") && await button("显示详情栏"));
  await until(() => button("最大化", true), "Maximize was missing");
  await until(maximized, "Window did not maximize");
  assert.equal((await close()).maximized, true);

  await start();
  await until(maximized, "Maximized window did not restore");
  await until(() => button("最大化", true), "Restore button was missing");
  await until(async () => !await maximized() && sameGeometry(await geometry(), restored), "Normal geometry did not restore");
  const image = join(process.env.SystemRoot ?? "C:\\Windows", "explorer.exe");
  const imported = await invoke("import_paths", { paths: [image], mode: "indexed", moveFiles: false, hint: null });
  const icon = imported.library.software.find((software) => software.id === imported.report.items[0]?.softwareId)?.iconData;
  assert.ok(icon?.startsWith("data:image/png;base64,"), "Installer icon not extracted");
  assert.ok(await session.webview.evaluate((src) => new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve(image.naturalWidth > 0);
    image.onerror = () => resolve(false);
    image.src = src;
  }), icon), "Extracted icon did not render");
  await testUninstaller();
  const old = await invoke("get_status");
  await invoke("set_repo_root", { path: join(temp, "next-repo") });
  await assert.rejects(invoke("fetch_summary", { name: "Stale request", revision: old.repoRevision }), /repo_changed/);
  await close();
  console.log("Windows desktop smoke passed: window and panels, icons, uninstaller and repository isolation.");
} finally {
  if (registered) {
    await nativeWindow("close", ["-Caption", caption]).catch(() => {});
    await runHidden("reg.exe", ["delete", registryKey, "/f"]).catch(() => {});
  }
  if (session?.child.exitCode === null) session.child.kill();
  session?.webview?.close();
  const resolvedTemp = await realpath(temp);
  const tempRoot = await realpath(tmpdir());
  if (dirname(resolvedTemp) === tempRoot && basename(resolvedTemp).startsWith("shibapack-native-smoke-")) {
    await rm(resolvedTemp, { recursive: true, force: true, maxRetries: 2 }).catch(() => {});
  }
}
