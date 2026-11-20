// 安装监控窗口。
//
// 1. 准备：选范围（标准 / 深度），提醒先关掉别的安装程序。
// 2. 记录基线：拍下安装前的样子，拍完自动运行安装包。
// 3. 安装中：显示过了多久、安装程序还在不在跑（看得出来的话）。装好后点“完成”；软件第一次打开还会写设置的话，可以先打开一次再点。
// 4. 对比：再拍一次，和基线比，存下记录。
// 5. 报告：按类别列出安装程序做的改动，开机启动、服务、驱动这类标出来；缓存、日志这类多半无关的默认藏起来。
//
// 也用来打开已有的记录（直接到第 5 步），和接着上次没完成的监控（安装程序要求重启了电脑，直接到第 3 步）。
//
// 安装中屏幕右上角另开一个总在最前面的小窗（TraceMini），装的时候不用切回 ShibaPack 也能看到进度、点“完成”。

import {
  AppWindow,
  BadgeCheck,
  CircleCheck,
  FileText,
  RotateCcw,
  Folder,
  FolderOpen,
  KeyRound,
  LoaderCircle,
  Radar,
  ShieldAlert,
  TriangleAlert,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, onTraceMiniAction, onTraceProgress } from "../lib/api";
import { errorText, type Key } from "../lib/i18n";
import { formatDate, formatSize } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { useApp, useLanguage, useT } from "../lib/store";
import { RISKY, TRACE_CATEGORIES, bumpTraces, closeTraceDialog, traceTotals, useTrace, type TraceTarget } from "../lib/trace";
import type { LiveSummary, Trace, TraceCategory, TraceChange, TraceProgress, TraceStatus } from "../lib/types";
import { confirm } from "./dialogs";
import { Button, Checkbox, MiddleTruncate, Modal, Segmented, cx } from "./ui";

type Stage = "prepare" | "baseline" | "installing" | "compare" | "report" | "loading" | "failed";

export function TraceDialog() {
  const target = useTrace((state) => state.target);
  if (!target) return null;
  return <Dialog key={target.mode === "view" ? target.traceId : target.mode === "start" ? target.fileId : "resume"} target={target} />;
}

function Dialog({ target }: { target: TraceTarget }) {
  const t = useT();
  const language = useLanguage();
  const deep = usePrefs((state) => state.traceDeep);
  const setPrefs = usePrefs((state) => state.set);
  const [stage, setStage] = useState<Stage>(target.mode === "view" ? "loading" : target.mode === "resume" ? "installing" : "prepare");
  const [progress, setProgress] = useState<TraceProgress | null>(null);
  const [status, setStatus] = useState<TraceStatus | null>(null);
  const [trace, setTrace] = useState<Trace | null>(null);
  const [error, setError] = useState<string | null>(null);

  // 打开已有的记录
  useEffect(() => {
    if (target.mode !== "view") return;
    api
      .getTrace(target.softwareId, target.traceId)
      .then((record) => {
        setTrace(record);
        setStage("report");
      })
      .catch((reason) => {
        setError(errorText(language, reason));
        setStage("failed");
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 拍快照时听进度
  useEffect(() => {
    if (stage !== "baseline" && stage !== "compare") return;
    let stop: (() => void) | undefined;
    let gone = false;
    void onTraceProgress(setProgress).then((unlisten) => (gone ? unlisten() : (stop = unlisten)));
    return () => {
      gone = true;
      stop?.();
    };
  }, [stage]);

  // 安装中每秒看一次：安装程序还在不在跑、实时改动
  useEffect(() => {
    if (stage !== "installing") return;
    let alive = true;
    const poll = () =>
      void api.traceStatus().then((next) => {
        if (alive && next) setStatus(next);
      });
    poll();
    const timer = setInterval(poll, 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [stage]);

  // 浮动小窗由后端开关（开始监控时开，完成或放弃时关）；小窗上点“完成”时这里接着做
  const finishRef = useRef<() => void>(() => {});
  useEffect(() => {
    if (stage !== "installing") return;
    let stop: (() => void) | undefined;
    let gone = false;
    void onTraceMiniAction((action) => action === "finish" && finishRef.current()).then((unlisten) => (gone ? unlisten() : (stop = unlisten)));
    return () => {
      gone = true;
      stop?.();
    };
  }, [stage]);

  const start = async () => {
    if (target.mode !== "start") return;
    setProgress(null);
    setStage("baseline");
    try {
      await api.traceStart({ softwareId: target.softwareId, versionId: target.versionId, fileId: target.fileId }, deep);
      setStage("installing");
    } catch (reason) {
      if (String(reason) === "cancelled") {
        setStage("prepare");
        return;
      }
      setError(errorText(language, reason));
      setStage("failed");
    }
  };

  const finish = async () => {
    setProgress(null);
    setStage("compare");
    try {
      const record = await api.traceFinish();
      setTrace(record);
      setStage("report");
      bumpTraces();
    } catch (reason) {
      if (String(reason) === "cancelled") {
        setStage("installing");
        return;
      }
      setError(errorText(language, reason));
      setStage("failed");
    }
  };

  finishRef.current = () => void finish();

  const abandon = async () => {
    if (stage === "installing") {
      const { ok } = await confirm({ title: t("trace.abandonTitle"), body: t("trace.abandonBody"), confirmLabel: t("trace.abandon"), danger: true });
      if (!ok) return;
    }
    await api.traceCancel();
    closeTraceDialog();
  };

  const close = () => {
    if (stage === "baseline" || stage === "compare") return;
    if (stage === "installing") return void abandon();
    closeTraceDialog();
  };

  const remove = async () => {
    if (!trace) return;
    const { ok } = await confirm({ title: t("trace.deleteTitle"), body: t("trace.deleteBody"), confirmLabel: t("trace.delete"), danger: true });
    if (!ok) return;
    try {
      await api.deleteTrace(trace.softwareId, trace.id);
      bumpTraces();
      closeTraceDialog();
    } catch (reason) {
      useApp.getState().notify(errorText(language, reason), "error");
    }
  };

  const name =
    target.mode === "start"
      ? `${target.softwareName} ${target.version}`.trim()
      : trace
        ? `${trace.softwareName} ${trace.version}`.trim()
        : status
          ? `${status.softwareName} ${status.version}`.trim()
          : "";
  const title = stage === "report" || stage === "loading" ? t("trace.reportTitle", { name }) : t("trace.title", { name });

  let footer;
  if (stage === "prepare") {
    footer = (
      <>
        <Button variant="ghost" onClick={closeTraceDialog}>
          {t("common.cancel")}
        </Button>
        <Button variant="primary" onClick={() => void start()}>
          {t("trace.start")}
        </Button>
      </>
    );
  } else if (stage === "baseline" || stage === "compare") {
    footer = (
      <Button variant="ghost" onClick={() => void api.traceCancel()}>
        {stage === "baseline" ? t("common.cancel") : t("trace.stopCompare")}
      </Button>
    );
  } else if (stage === "installing") {
    footer = (
      <>
        <Button variant="ghost" className="mr-auto" onClick={() => void abandon()}>
          {t("trace.abandon")}
        </Button>
        <Button variant="primary" onClick={() => void finish()}>
          {t("trace.finish")}
        </Button>
      </>
    );
  } else if (stage === "report" && trace) {
    footer = (
      <>
        <Button variant="caution" className="mr-auto" onClick={() => void remove()}>
          {t("trace.delete")}
        </Button>
        <Button variant="primary" onClick={closeTraceDialog}>
          {t("common.close")}
        </Button>
      </>
    );
  } else {
    footer = (
      <Button variant="primary" onClick={closeTraceDialog}>
        {t("common.close")}
      </Button>
    );
  }

  return (
    <Modal open onClose={close} title={title} width={stage === "report" ? 760 : 460} footer={footer}>
      {stage === "prepare" && target.mode === "start" && (
        <div className="flex flex-col gap-4">
          <p className="m-0 text-[13px] leading-relaxed text-fg-2">{t("trace.intro", { file: target.fileName })}</p>
          <div>
            <div className="mb-1.5 text-[12px] font-medium text-fg-2">{t("trace.depth")}</div>
            <Segmented
              value={deep ? "deep" : "standard"}
              options={[
                { value: "standard", label: t("trace.depth.standard") },
                { value: "deep", label: t("trace.depth.deep") },
              ]}
              onChange={(value) => setPrefs({ traceDeep: value === "deep" })}
            />
            <p className="m-0 mt-1.5 text-[12px] leading-relaxed text-fg-3">{t(deep ? "trace.depth.deepHint" : "trace.depth.standardHint")}</p>
          </div>
          <p className="m-0 flex items-start gap-1.5 text-[12px] leading-relaxed text-warning">
            <TriangleAlert size={13} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
            {t("trace.closeOthers")}
          </p>
        </div>
      )}

      {(stage === "baseline" || stage === "compare") && (
        <div className="flex items-center gap-3 py-4">
          <LoaderCircle size={20} strokeWidth={1.75} className="shrink-0 animate-spin text-accent" aria-hidden />
          <div className="min-w-0">
            <div className="text-[13px] font-medium">{t(stage === "baseline" ? "trace.baseline" : "trace.comparing")}</div>
            <div className="tabular text-[12px] text-fg-3">{t("trace.entries", { n: (progress?.entries ?? 0).toLocaleString(language) })}</div>
          </div>
        </div>
      )}

      {stage === "installing" && <Installing status={status} />}

      {stage === "loading" && (
        <div className="flex items-center gap-2 py-6 text-[13px] text-fg-3">
          <LoaderCircle size={16} strokeWidth={1.75} className="animate-spin" aria-hidden />
          {t("trace.loading")}
        </div>
      )}

      {stage === "failed" && (
        <p className="m-0 flex items-start gap-1.5 py-2 text-[13px] leading-relaxed text-danger">
          <TriangleAlert size={14} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
          {error}
        </p>
      )}

      {stage === "report" && trace && <Report trace={trace} />}
    </Modal>
  );
}

function Installing({ status }: { status: TraceStatus | null }) {
  const t = useT();
  const elapsed = Math.floor((status?.elapsedMs ?? 0) / 1000);
  const clock = `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
  const running = status?.installerRunning;
  return (
    <div className="flex flex-col gap-3">
      {status?.resumed && (
        <p className="m-0 flex items-start gap-1.5 rounded-md bg-accent-soft px-2.5 py-2 text-[12px] leading-relaxed text-accent">
          <RotateCcw size={13} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
          {t("trace.resumed")}
        </p>
      )}
      <div className="flex items-center gap-3 rounded-lg border border-line px-3 py-3">
        <span className="relative flex size-8 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent">
          <Radar size={16} strokeWidth={1.75} aria-hidden />
          {running !== false && <span className="absolute inset-0 animate-ping rounded-full bg-accent-soft" aria-hidden />}
        </span>
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium">{t("trace.watching")}</div>
          <div className="truncate text-[12px] text-fg-3">{status?.fileName}</div>
        </div>
        <span className="tabular shrink-0 text-[13px] text-fg-2">{clock}</span>
      </div>
      {status?.live && <LiveView live={status.live} />}
      {status?.rebootRequired ? (
        <p className="m-0 flex items-start gap-1.5 text-[13px] leading-relaxed text-warning">
          <RotateCcw size={14} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
          {t("trace.rebootRequired")}
        </p>
      ) : running === false ? (
        <p className="m-0 flex items-start gap-1.5 text-[13px] leading-relaxed text-success">
          <CircleCheck size={14} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
          {t("trace.installerExited")}
        </p>
      ) : (
        <p className="m-0 text-[13px] leading-relaxed text-fg-2">{t(running ? "trace.installerRunning" : "trace.installerUnknown")}</p>
      )}
      <p className="m-0 text-[12px] leading-relaxed text-fg-3">{t("trace.firstRunHint")}</p>
    </div>
  );
}

/** 实时改动：各类别动了几个文件，和最近的几次改动。 */
export function LiveView({ live, compact = false }: { live: LiveSummary; compact?: boolean }) {
  const t = useT();
  const present = TRACE_CATEGORIES.filter((cat) => (live.counts[cat] ?? 0) > 0);
  const recent = compact ? live.recent.slice(0, 2) : live.recent;
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12px]">
        <span className="font-medium">{t("trace.liveFiles", { n: live.files })}</span>
        {present.map((cat) => (
          <span key={cat} className={cx(RISKY.includes(cat) ? "text-warning" : "text-fg-3")}>
            {t(`trace.cat.${cat}` as Key)} <span className="tabular">{live.counts[cat]}</span>
          </span>
        ))}
      </div>
      {recent.length > 0 && (
        <div className={cx("flex flex-col", !compact && "rounded-lg border border-line px-2 py-1")}>
          {recent.map((event, i) => (
            <div key={`${event.atMs}-${i}`} className="flex h-6 items-center gap-1.5">
              <span className={cx("shrink-0 rounded px-1 text-[12px] leading-[18px]", OP_STYLE[event.op])}>{t(`trace.op.${event.op}` as Key)}</span>
              <MiddleTruncate text={event.path} tail={compact ? 18 : 28} className="text-[12px] text-fg-2" />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const OP_STYLE: Record<TraceChange["op"], string> = {
  added: "bg-[color-mix(in_srgb,var(--success)_16%,transparent)] text-success",
  modified: "bg-[color-mix(in_srgb,var(--warning)_16%,transparent)] text-warning",
  removed: "bg-[color-mix(in_srgb,var(--danger)_16%,transparent)] text-danger",
};

/** 每个分组最多先画这么多行，多的点“显示全部”。 */
const PAGE = 200;

function Report({ trace }: { trace: Trace }) {
  const t = useT();
  const language = useLanguage();
  const attempt = useApp((state) => state.attempt);
  const [showNoise, setShowNoise] = useState(false);
  const [category, setCategory] = useState<TraceCategory | "all">("all");
  const [expanded, setExpanded] = useState<Set<TraceCategory>>(new Set());
  const totals = useMemo(() => traceTotals(trace.changes), [trace]);
  const windows = trace.platform === "windows";

  // 带上原来的序号：定位时按序号找
  const indexed = useMemo(() => trace.changes.map((change, index) => ({ change, index })), [trace]);
  const visible = indexed.filter(({ change }) => (showNoise || !change.noise) && (category === "all" || change.category === category));
  const groups = TRACE_CATEGORIES.map((cat) => ({ cat, rows: visible.filter(({ change }) => change.category === cat) })).filter((g) => g.rows.length > 0);
  const risky = RISKY.filter((cat) => (totals.byCategory[cat] ?? 0) > 0);
  const present = TRACE_CATEGORIES.filter((cat) => (totals.byCategory[cat] ?? 0) > 0);
  const duration = Math.max(0, Math.round((new Date(trace.finishedAt).getTime() - new Date(trace.startedAt).getTime()) / 1000));

  return (
    <div className="flex flex-col gap-3">
      <div className="text-[12px] text-fg-3">
        {[
          trace.fileName,
          formatDate(trace.startedAt, language, true),
          t("trace.took", { m: Math.floor(duration / 60), s: duration % 60 }),
          t(trace.depth === "deep" ? "trace.depth.deep" : "trace.depth.standard"),
        ].join(" · ")}
      </div>

      {(trace.resumed || trace.rebootRequired) && (
        <p className="m-0 flex items-start gap-1.5 text-[12px] leading-relaxed text-fg-2">
          <RotateCcw size={13} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0 text-fg-3" />
          {t(trace.resumed ? "trace.reportResumed" : "trace.reportReboot")}
        </p>
      )}

      <div className="grid grid-cols-4 gap-2">
        <Stat label={t("trace.stat.added")} value={formatSize(totals.addedBytes)} />
        <Stat label={t("trace.stat.files")} value={totals.files.toLocaleString(language)} />
        {windows ? (
          <Stat label={t("trace.stat.registry")} value={totals.registry.toLocaleString(language)} />
        ) : (
          <Stat label={t("trace.stat.settings")} value={String(totals.byCategory.settings ?? 0)} />
        )}
        <Stat
          label={t("trace.stat.background")}
          value={String((totals.byCategory.startup ?? 0) + (totals.byCategory.service ?? 0) + (totals.byCategory.driver ?? 0))}
          warn={risky.length > 0}
        />
      </div>

      {risky.length > 0 && (
        <p className="m-0 flex items-start gap-1.5 rounded-md bg-[color-mix(in_srgb,var(--warning)_12%,transparent)] px-2.5 py-2 text-[12px] leading-relaxed text-warning">
          <ShieldAlert size={14} strokeWidth={1.75} aria-hidden className="mt-px shrink-0" />
          {t("trace.risk", { list: risky.map((cat) => t(`trace.cat.${cat}` as Key)).join(t("common.listSeparator")) })}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-1">
        <Chip active={category === "all"} onClick={() => setCategory("all")}>
          {t("trace.all")}
        </Chip>
        {present.map((cat) => (
          <Chip key={cat} active={category === cat} onClick={() => setCategory(cat)}>
            {t(`trace.cat.${cat}` as Key)} <span className="tabular text-fg-3">{totals.byCategory[cat]}</span>
          </Chip>
        ))}
        {totals.noise > 0 && (
          <div className="ml-auto">
            <Checkbox checked={showNoise} onChange={setShowNoise}>
              <span className="text-[12px] text-fg-2">{t("trace.showNoise", { n: totals.noise })}</span>
            </Checkbox>
          </div>
        )}
      </div>

      {groups.length === 0 ? (
        <p className="m-0 py-6 text-center text-[13px] text-fg-3">{t(trace.changes.length === 0 ? "trace.nothing" : "trace.nothingShown")}</p>
      ) : (
        <div className="flex flex-col gap-3">
          {groups.map(({ cat, rows }) => {
            const all = expanded.has(cat);
            const shown = all ? rows : rows.slice(0, PAGE);
            return (
              <section key={cat}>
                <h3 className={cx("m-0 mb-1 flex items-center gap-1.5 text-[12px] font-semibold", RISKY.includes(cat) ? "text-warning" : "text-fg-2")}>
                  {t(`trace.cat.${cat}` as Key)}
                  <span className="tabular font-normal text-fg-3">{rows.length}</span>
                </h3>
                <div className="rounded-lg border border-line">
                  {shown.map(({ change, index }) => (
                    <Row key={index} change={change} onReveal={() => void attempt(() => api.traceReveal(trace.softwareId, trace.id, index))} />
                  ))}
                  {!all && rows.length > PAGE && (
                    <button
                      type="button"
                      onClick={() => setExpanded((set) => new Set(set).add(cat))}
                      className="h-8 w-full text-[12px] text-accent hover:bg-hover"
                    >
                      {t("trace.showAll", { n: rows.length - PAGE })}
                    </button>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      )}
      {(trace.products?.length ?? 0) > 0 && (
        <section>
          <h3 className="m-0 mb-1 flex items-center gap-1.5 text-[12px] font-semibold text-fg-2">
            <BadgeCheck size={13} strokeWidth={1.75} aria-hidden className="text-accent" />
            {t("trace.products")}
          </h3>
          <p className="m-0 mb-1.5 text-[12px] leading-relaxed text-fg-3">{t("trace.productsHint")}</p>
          <div className="rounded-lg border border-line">
            {trace.products!.map((product) => (
              <div key={product.id} className="flex min-h-8 items-center gap-2 border-b border-line px-2.5 py-1 last:border-b-0">
                <span className="shrink-0 rounded bg-hover px-1.5 text-[12px] leading-5 text-fg-2 uppercase">{product.kind}</span>
                <span className="selectable min-w-0 flex-1 truncate text-[12px]">
                  {product.name !== product.id ? `${product.name} ` : ""}
                  <span className="text-fg-3">{product.id}</span>
                </span>
                {product.version && <span className="shrink-0 text-[12px] text-fg-3">{product.version}</span>}
                <span className="tabular shrink-0 text-[12px] text-fg-3">{t("trace.items", { n: product.files })}</span>
              </div>
            ))}
          </div>
        </section>
      )}
      {(trace.transientCount ?? 0) > 0 && <TransientList trace={trace} />}
      {trace.truncated && <p className="m-0 text-[12px] text-fg-3">{t("trace.truncated")}</p>}
    </div>
  );
}

/** 安装期间出现过、装完已经不在的文件（默认收起）。 */
function TransientList({ trace }: { trace: Trace }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const list = trace.transient ?? [];
  const count = trace.transientCount ?? list.length;
  return (
    <section>
      <button type="button" aria-expanded={open} onClick={() => setOpen(!open)} className="flex h-7 items-center gap-1 text-[12px] font-semibold text-fg-2 hover:text-fg">
        {t("trace.transient", { n: count })}
        <span className={cx("text-fg-3 transition-transform", open && "rotate-90")}>›</span>
      </button>
      {open && (
        <>
          <p className="m-0 mb-1.5 text-[12px] leading-relaxed text-fg-3">{t("trace.transientHint")}</p>
          <div className="rounded-lg border border-line">
            {list.map((item) => (
              <div key={item.path} className="flex h-7 items-center gap-2 border-b border-line px-2.5 last:border-b-0">
                <FileText size={13} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
                <MiddleTruncate text={item.path} tail={28} className={cx("selectable text-[12px]", item.noise && "text-fg-3")} />
              </div>
            ))}
            {count > list.length && <p className="m-0 px-2.5 py-1.5 text-[12px] text-fg-3">{t("trace.transientMore", { n: count - list.length })}</p>}
          </div>
        </>
      )}
    </section>
  );
}

function Stat({ label, value, warn }: { label: string; value: string; warn?: boolean }) {
  return (
    <div className="rounded-lg border border-line px-3 py-2">
      <div className="text-[12px] text-fg-3">{label}</div>
      <div className={cx("tabular text-[16px] font-semibold", warn && "text-warning")}>{value}</div>
    </div>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={cx(
        "inline-flex h-6 items-center gap-1 rounded-full px-2.5 text-[12px] transition-colors",
        active ? "bg-accent-soft text-accent" : "text-fg-2 hover:bg-hover",
      )}
    >
      {children}
    </button>
  );
}

function Row({ change, onReveal }: { change: TraceChange; onReveal: () => void }) {
  const t = useT();
  const isFile = change.kind === "file" || change.kind === "folder";
  const Icon = change.kind === "folder" ? (change.path.toLowerCase().endsWith(".app") ? AppWindow : Folder) : change.kind === "file" ? FileText : KeyRound;
  const detail: string[] = [];
  if (change.value !== undefined) detail.push(change.value === "" ? t("trace.defaultValue") : change.value);
  if (change.kind === "registryKey" && change.count) detail.push(t("trace.items", { n: change.count }));
  if (change.kind === "folder" && change.count && change.op !== "modified") detail.push(t("trace.items", { n: change.count }));

  return (
    <div className="group flex min-h-8 items-center gap-2 border-b border-line px-2.5 py-1 last:border-b-0 hover:bg-hover">
      <span className={cx("shrink-0 rounded px-1.5 text-[12px] leading-5", OP_STYLE[change.op])}>{t(`trace.op.${change.op}` as Key)}</span>
      <Icon size={13} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
      <div className="min-w-0 flex-1">
        <MiddleTruncate text={change.path} tail={28} className={cx("selectable text-[12px]", change.noise && "text-fg-3")} />
        {(detail.length > 0 || change.data) && (
          <div className="truncate text-[12px] text-fg-3" title={change.data}>
            {detail.join(" · ")}
            {change.data && <span className="selectable">{detail.length > 0 ? " = " : ""}{change.data}</span>}
          </div>
        )}
      </div>
      {change.confirmed && (
        <span title={t("trace.confirmedHint")} className="flex shrink-0 text-accent">
          <BadgeCheck size={13} strokeWidth={1.75} aria-label={t("trace.confirmed")} />
        </span>
      )}
      {change.size !== undefined && change.op !== "removed" && <span className="tabular shrink-0 text-[12px] text-fg-3">{formatSize(change.size)}</span>}
      {isFile && change.op !== "removed" && (
        <button
          type="button"
          title={t("detail.reveal")}
          aria-label={t("detail.reveal")}
          onClick={onReveal}
          className="flex size-6 shrink-0 items-center justify-center rounded-md text-fg-3 opacity-0 group-hover:opacity-100 hover:bg-active hover:text-fg focus:opacity-100"
        >
          <FolderOpen size={13} strokeWidth={1.75} aria-hidden />
        </button>
      )}
    </div>
  );
}
