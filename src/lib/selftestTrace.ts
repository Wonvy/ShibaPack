// 安装监控的自检：在真实的桌面窗口里走一遍“监控安装”，结果打印到启动它的终端。
//
// 自检文件名里带 `trace-selftest` 时只跑这一项（见 selftest.ts）。安装包用一个没有内容的 .pkg：
//   pkgbuild --nopayload --identifier com.shibapack.tracetest --version 1 trace-selftest.pkg
// 它只会打开“安装器”，不装任何东西。安装期间的改动由外面的脚本来做：看到 `TRACE_READY` 后，
// 在 ~/Library/Application Support/ShibaPackTraceTest 里建文件、建一个马上删掉的临时文件、
// 写一个 com.shibapack.tracetest 的设置，然后退出“安装器”。
//
// 两种走法：
// - 没有没完成的监控：导入安装包、从监控窗口开始、等改动、等安装器退出、从小窗点“完成”、检查报告。
//   在 `TRACE_READY` 之后把应用杀掉，就留下一次没完成的监控。
// - 启动时有没完成的监控（上面那样杀掉以后再启动）：检查它是接着来的，等改动，完成，检查报告。

import { api } from "./api";
import { closeTraceDialog, openTraceDialog, useTrace } from "./trace";
import type { Trace, TraceStatus } from "./types";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** 每 300 毫秒看一次，直到满足条件或超时。返回最后一次看到的值。 */
async function until<T>(read: () => Promise<T>, ok: (value: T) => boolean, timeoutMs: number): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let value = await read();
  while (!ok(value) && Date.now() < deadline) {
    await sleep(300);
    value = await read();
  }
  return value;
}

const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("[role=dialog] button")].find((b) => b.textContent?.trim() === text);

async function miniOpen() {
  const { Window } = await import("@tauri-apps/api/window");
  return (await Window.getByLabel("trace-mini")) !== null;
}

export async function runTraceSelftest(report: (line: string) => Promise<void>, check: (name: string, ok: boolean, detail?: string) => Promise<void>, installer: string) {
  const pending = await api.traceStatus();
  let softwareId: string;

  if (pending) {
    await check("启动时接着上次没完成的监控", pending.resumed, `${pending.softwareName} · ${Math.round(pending.elapsedMs / 1000)} 秒前开始`);
    const opened = await until(async () => useTrace.getState().target, (target) => target?.mode === "resume", 5000);
    await check("自动打开监控窗口", opened?.mode === "resume");
    softwareId = pending.softwareId;
  } else {
    const imported = await api.importPaths([installer], "managed");
    const item = imported.report.items[0];
    const software = imported.library.software.find((s) => s.id === item?.softwareId);
    const version = software?.versions[0];
    const file = version?.files[0];
    if (!software || !version || !file) {
      await check("导入测试安装包", false, `${item?.status} ${item?.message ?? ""}`);
      return;
    }
    softwareId = software.id;
    openTraceDialog({ mode: "start", softwareId, versionId: version.id, fileId: file.id, fileName: file.name, softwareName: software.name, version: version.version });
    await sleep(500);
    const start = button("开始监控") ?? button("Start");
    if (!start) {
      await check("监控窗口里有“开始监控”", false);
      return;
    }
    const began = Date.now();
    start.click();
    const status = await until(api.traceStatus, (s) => s !== null, 120_000);
    await check("记录基线并运行安装包", status !== null, `${Date.now() - began} ms`);
    if (!status) return;
    const running = await until(api.traceStatus, (s) => s?.installerRunning === true, 15_000);
    await check("看得出安装器在运行", running?.installerRunning === true, String(running?.installerRunning));
    await check("实时监听已开启", running?.live !== null);
  }

  await sleep(1000);
  await check("浮动小窗已打开", await miniOpen());
  await report("TRACE_READY");

  // 等外面的脚本做改动
  // 别的程序也会在后台写文件，所以等的是脚本写的那几个
  const scripted = (s: TraceStatus | null) => Boolean(s?.live?.recent.some((e) => e.path.includes("com.shibapack.tracetest")));
  const live = await until(api.traceStatus, scripted, 90_000);
  await sleep(1500);
  await check("实时看到改动", scripted(live) && (live?.live?.files ?? 0) >= 2, `${live?.live?.files} 个文件 · 最近：${live?.live?.recent.map((e) => e.path.split("/").pop()).join("、")}`);

  if (!pending) {
    const exited = await until(api.traceStatus, (s: TraceStatus | null) => s?.installerRunning === false, 60_000);
    await check("看得出安装器退出了", exited?.installerRunning === false);
  }

  // 从小窗点“完成”：主窗口收到后接着完成
  const before = (await api.listTraces(softwareId)).length;
  await api.traceMiniAction("finish");
  const traces = await until(() => api.listTraces(softwareId), (list) => list.length > before, 120_000);
  await check("从小窗点完成，存下了记录", traces.length > before);
  if (traces.length <= before) return;
  const trace: Trace = await api.getTrace(softwareId, traces[0].id);
  const folder = trace.changes.find((c) => c.path.endsWith("/ShibaPackTraceTest"));
  await check("报告里有新增的文件夹", folder?.op === "added" && folder.kind === "folder", folder ? `${folder.count} 项 · ${folder.size} B · ${folder.category}` : "没有");
  const plist = trace.changes.find((c) => c.path.endsWith("com.shibapack.tracetest.plist"));
  await check("报告里有新增的设置", plist?.op === "added" && plist.category === "settings", plist?.path ?? "没有");
  const temp = trace.transient?.find((t) => t.path.endsWith("installing.tmp"));
  await check("报告里有临时文件，不算噪音", Boolean(temp) && !temp?.noise, `${trace.transientCount} 个`);
  await check("接着来的监控在报告里有标记", trace.resumed === Boolean(pending));
  const real = trace.changes.filter((c) => !c.noise);
  await report(`  报告：${trace.changes.length} 项改动（不算噪音 ${real.length} 项），基线 ${trace.baselineMs} ms，对比 ${trace.compareMs} ms，记了 ${trace.entries} 项`);
  for (const c of real.slice(0, 12)) await report(`    ${c.op} ${c.category} ${c.path}`);
  for (const t of trace.transient ?? []) await report(`    临时 ${t.noise ? "(噪音) " : ""}${t.path}`);

  await sleep(800);
  await check("监控结束后小窗关掉了", !(await miniOpen()));
  await check("没完成的监控已经清掉", (await api.traceStatus()) === null);
  closeTraceDialog();
  await api.deleteTrace(softwareId, trace.id);
  await check("删除记录", (await api.listTraces(softwareId)).every((t) => t.id !== trace.id));
}
