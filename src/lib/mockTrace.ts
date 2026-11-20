// 浏览器预览用的安装监控：假装拍快照、假装安装，装完给一份像样的记录。

import type { LiveEvent, LiveSummary, Trace, TraceChange, TraceProgress, TraceStatus, TraceSummary } from "./types";

const listeners = new Set<(progress: TraceProgress) => void>();
const records: Trace[] = [];
let session: { softwareId: string; versionId: string; fileName: string; started: number } | null = null;
let cancelled = false;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function mockOnTrace(handler: (progress: TraceProgress) => void) {
  listeners.add(handler);
  return () => listeners.delete(handler);
}

async function fakeScan(stage: TraceProgress["stage"]) {
  cancelled = false;
  for (let i = 1; i <= 12; i++) {
    await sleep(120);
    if (cancelled) throw "cancelled";
    listeners.forEach((l) => l({ stage, entries: i * 18_342 }));
  }
}

const mac = !/Windows/i.test(navigator.userAgent);

function fakeChanges(name: string): TraceChange[] {
  const slug = name.replace(/\s+/g, "");
  if (mac) {
    return [
      { op: "added", kind: "folder", path: `/Applications/${name}.app`, size: 248_000_000, count: 1834, category: "app", noise: false, confirmed: true },
      { op: "added", kind: "file", path: `/Library/LaunchDaemons/com.${slug.toLowerCase()}.helper.plist`, size: 612, category: "service", noise: false, confirmed: true },
      { op: "added", kind: "file", path: `/Library/PrivilegedHelperTools/com.${slug.toLowerCase()}.helper`, size: 1_204_000, category: "service", noise: false, confirmed: true },
      { op: "added", kind: "file", path: `/Users/me/Library/LaunchAgents/com.${slug.toLowerCase()}.updater.plist`, size: 540, category: "startup", noise: false },
      { op: "added", kind: "folder", path: `/Users/me/Library/Application Support/${name}`, size: 3_400_000, count: 42, category: "data", noise: false },
      { op: "added", kind: "file", path: `/Users/me/Library/Preferences/com.${slug.toLowerCase()}.plist`, size: 2_048, category: "settings", noise: false },
      { op: "added", kind: "file", path: `/private/var/db/receipts/com.${slug.toLowerCase()}.pkg.plist`, size: 1_210, category: "uninstall", noise: false },
      { op: "added", kind: "file", path: "/usr/local/bin/" + slug.toLowerCase(), size: 58, category: "app", noise: false, confirmed: true },
      { op: "modified", kind: "file", path: "/Users/me/Library/Preferences/com.apple.dock.plist", size: 12_204, category: "settings", noise: true },
      { op: "added", kind: "folder", path: `/Users/me/Library/Caches/com.${slug.toLowerCase()}`, size: 8_102_000, count: 211, category: "data", noise: true },
      { op: "modified", kind: "file", path: "/Users/me/Library/Application Support/Google/Chrome/Default/History", size: 4_202_000, category: "data", noise: true },
    ];
  }
  return [
    { op: "added", kind: "folder", path: `C:\\Program Files\\${name}`, size: 312_000_000, count: 960, category: "app", noise: false, confirmed: true },
    { op: "added", kind: "file", path: `C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\${name}.lnk`, size: 2_104, category: "shortcut", noise: false },
    { op: "added", kind: "file", path: `C:\\Users\\Public\\Desktop\\${name}.lnk`, size: 2_104, category: "shortcut", noise: false },
    { op: "added", kind: "registryValue", path: "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", value: name, data: `"C:\\Program Files\\${name}\\${slug}.exe" --background`, category: "startup", noise: false },
    { op: "added", kind: "registryKey", path: `HKLM\\SYSTEM\\CurrentControlSet\\Services\\${slug}Update`, count: 9, category: "service", noise: false },
    { op: "added", kind: "registryKey", path: `HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\${name}`, count: 12, category: "uninstall", noise: false },
    { op: "added", kind: "registryKey", path: `HKCU\\Software\\Classes\\.${slug.toLowerCase()}`, count: 3, category: "association", noise: false },
    { op: "modified", kind: "registryValue", path: "HKCU\\Environment", value: "Path", data: `C:\\Users\\me\\AppData\\Local\\Microsoft\\WindowsApps;C:\\Program Files\\${name}\\bin`, category: "environment", noise: false },
    { op: "added", kind: "registryKey", path: `HKLM\\SOFTWARE\\${name}`, count: 27, category: "settings", noise: false },
    { op: "added", kind: "folder", path: `C:\\Users\\me\\AppData\\Roaming\\${name}`, size: 1_900_000, count: 18, category: "data", noise: false },
    { op: "added", kind: "folder", path: `C:\\Users\\me\\AppData\\Local\\Temp\\${slug}-setup`, size: 92_000_000, count: 31, category: "data", noise: true },
  ];
}

/** 装得越久动的文件越多。 */
function fakeLive(elapsedMs: number): LiveSummary {
  const n = Math.min(1900, Math.floor(elapsedMs / 4));
  const base = mac ? "/Applications/Demo App.app/Contents/" : "C:\\Program Files\\Demo App\\";
  const recent: LiveEvent[] = [0, 1, 2, 3].map((i) => ({
    op: "added",
    path: `${base}${mac ? "Resources/" : ""}file-${n - i}.dat`,
    category: "app",
    atMs: elapsedMs - i * 40,
  }));
  if (elapsedMs > 3000) recent.unshift({ op: "added", path: mac ? "/Library/LaunchDaemons/com.demoapp.helper.plist" : "C:\\Windows\\System32\\drivers\\demo.sys", category: mac ? "service" : "driver", atMs: elapsedMs });
  return {
    files: n + (elapsedMs > 3000 ? 3 : 0),
    counts: { app: n, ...(elapsedMs > 3000 ? { service: 2, startup: 1 } : {}) },
    noise: Math.floor(n / 10),
    recent: recent.slice(0, 6),
  };
}

export async function mockTraceCall(command: string, args: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case "trace_start": {
      if (session) throw "trace_busy";
      await fakeScan("baseline");
      session = { softwareId: String(args.softwareId), versionId: String(args.versionId), fileName: "setup.pkg", started: Date.now() };
      return null;
    }
    case "trace_status": {
      if (!session) return null;
      const elapsedMs = Date.now() - session.started;
      const status: TraceStatus = {
        softwareId: session.softwareId,
        versionId: session.versionId,
        softwareName: "Demo App",
        version: "2.4.1",
        fileName: session.fileName,
        elapsedMs,
        installerRunning: elapsedMs < 6000,
        live: fakeLive(elapsedMs),
        rebootRequired: false,
        resumed: false,
      };
      return status;
    }
    case "trace_finish": {
      if (!session) throw "not_found";
      await fakeScan("compare");
      const now = new Date().toISOString();
      const trace: Trace = {
        id: Math.random().toString(16).slice(2, 14),
        softwareId: session.softwareId,
        versionId: session.versionId,
        softwareName: "Demo App",
        version: "2.4.1",
        fileName: session.fileName,
        platform: mac ? "macos" : "windows",
        depth: "standard",
        startedAt: new Date(session.started).toISOString(),
        finishedAt: now,
        baselineMs: 1440,
        compareMs: 1520,
        entries: 220_104,
        changes: fakeChanges("Demo App"),
        truncated: false,
        products: mac
          ? [{ kind: "pkg", id: "com.demoapp.pkg", name: "com.demoapp.pkg", version: "2.4.1", files: 1842 }]
          : [{ kind: "msi", id: "{AC76BA86-7AD7-1033-7B44-AC0F074E4100}", name: "Demo App", version: "2.4.1", files: 960 }],
        transient: [
          { path: mac ? "/Users/me/Library/Application Support/Demo App/update.tmp" : "C:\\ProgramData\\Demo App\\setup.tmp", category: "data", noise: false },
          { path: mac ? "/Users/me/Library/Caches/com.demoapp/unpack/payload.bin" : "C:\\Users\\me\\AppData\\Local\\Temp\\DemoSetup\\payload.cab", category: "data", noise: true },
        ],
        transientCount: 2,
        rebootRequired: false,
        resumed: false,
      };
      records.unshift(trace);
      session = null;
      return trace;
    }
    case "trace_cancel":
      cancelled = true;
      session = null;
      return null;
    case "list_traces": {
      const id = args.softwareId as string | null;
      return records
        .filter((r) => !id || r.softwareId === id)
        .map((r): TraceSummary => {
          const real = r.changes.filter((c) => !c.noise);
          return {
            id: r.id,
            softwareId: r.softwareId,
            versionId: r.versionId,
            version: r.version,
            fileName: r.fileName,
            platform: r.platform,
            startedAt: r.startedAt,
            finishedAt: r.finishedAt,
            changes: real.length,
            addedBytes: real.filter((c) => c.op === "added").reduce((sum, c) => sum + (c.size ?? 0), 0),
          };
        });
    }
    case "get_trace": {
      const found = records.find((r) => r.id === args.id);
      if (!found) throw "not_found";
      return found;
    }
    case "delete_trace": {
      const i = records.findIndex((r) => r.id === args.id);
      if (i >= 0) records.splice(i, 1);
      return null;
    }
    case "trace_reveal":
    case "trace_mini":
    case "trace_mini_action":
      return null;
    default:
      throw `mock: unknown command ${command}`;
  }
}
