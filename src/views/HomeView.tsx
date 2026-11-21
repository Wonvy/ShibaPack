// 主页：本机的配置、CPU / 内存 / 显卡 / 网络 / 硬盘读写 / 系统盘、占用最多的进程、软件的数量和最近装的软件。
// 点“查看全部”换成任务管理器（全部进程，可以搜索、排序、结束进程）。
//
// 数据只在这一页开着、窗口在前面时每 2 秒读一次（lib/system.ts），什么都不存。

import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, CircleX, Copy, Info, LayoutGrid, List, Search, Skull, SquareTerminal } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { confirm } from "../components/dialogs";
import { InstalledSoftwareIcon, SoftwareIcon } from "../components/SoftwareIcon";
import { Button, ContextMenu, IconButton, type MenuEntry, Modal, cx } from "../components/ui";
import { api } from "../lib/api";
import { errorText } from "../lib/i18n";
import { formatDate, formatSize, hasUpdate } from "../lib/library";
import { useCleanup } from "../lib/cleanup";
import { usePrefs } from "../lib/prefs";
import { useStartup } from "../lib/startup";
import { useApp, useLanguage, useT } from "../lib/store";
import { useSampling, useSystem } from "../lib/system";
import type { InstalledApp, ProcessInfo } from "../lib/types";

/** 超过这个百分比，数字和走势线变成警告色。 */
const BUSY = 90;

export function HomeView() {
  useSampling();
  const open = useSystem((state) => state.processesOpen);
  return open ? <ProcessesPage /> : <Dashboard />;
}

function Dashboard() {
  const t = useT();
  const language = useLanguage();
  const overview = useSystem((state) => state.overview);
  const sample = useSystem((state) => state.sample);
  const cpuHistory = useSystem((state) => state.cpuHistory);
  const memoryHistory = useSystem((state) => state.memoryHistory);
  const gpuHistory = useSystem((state) => state.gpuHistory);
  const downHistory = useSystem((state) => state.downHistory);
  const upHistory = useSystem((state) => state.upHistory);
  const readHistory = useSystem((state) => state.readHistory);
  const writeHistory = useSystem((state) => state.writeHistory);
  const error = useSystem((state) => state.error);
  const [details, setDetails] = useState(false);

  const memoryPercent = sample && sample.memoryTotal > 0 ? (sample.memoryUsed / sample.memoryTotal) * 100 : 0;
  const disk = sample?.disk ?? null;
  const diskPercent = disk && disk.total > 0 ? ((disk.total - disk.available) / disk.total) * 100 : 0;
  const gpu = sample?.gpu ?? null;
  const network = sample?.network ?? null;
  const diskIo = sample?.diskIo ?? null;
  const gpuNote = gpu
    ? gpu.memoryUsed !== null && gpu.memoryTotal
      ? t("home.gpuMemory", { used: formatSize(gpu.memoryUsed), total: formatSize(gpu.memoryTotal) })
      : gpu.model
    : sample && overview
      ? t(overview.gpus.length > 0 ? "home.gpuUnavailable" : "home.noGpu")
      : "";

  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      {/* 主页不要标题栏，直接是内容 */}
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pt-3 pb-16">
        {error !== null && !sample ? (
          <p className="m-0 rounded-lg border border-line px-3 py-2.5 text-[13px] text-danger">{errorText(language, error)}</p>
        ) : null}

        <Card className="mb-3 flex items-center gap-2 py-1.5 pr-1.5">
          <div className="flex min-w-0 flex-1 flex-wrap items-baseline gap-x-3 gap-y-1 py-1">
            <span className="truncate text-[14px] font-semibold">{overview?.hostName || t("home.thisComputer")}</span>
            <span className="min-w-0 truncate text-[12px] text-fg-3">
              {overview
                ? [
                    overview.os,
                    overview.cpuBrand && `${overview.cpuBrand} · ${t("home.cores", { n: overview.cpuCores ?? overview.cpuThreads })}`,
                    t("home.memoryTotal", { size: formatSize(overview.totalMemory) }),
                    sample && t("home.uptime", { time: duration(sample.uptime, t) }),
                  ]
                    .filter(Boolean)
                    .join(" · ")
                : t("home.loading")}
            </span>
          </div>
          <IconButton label={t("home.hardware")} disabled={!overview} onClick={() => setDetails(true)}>
            <Info size={15} strokeWidth={1.75} />
          </IconButton>
        </Card>
        {details && overview && <HardwareDialog onClose={() => setDetails(false)} />}

        {/* 两排三张：CPU、内存、显卡；网络、硬盘读写、系统盘 */}
        <div className="mb-3 grid grid-cols-1 gap-3 @[400px]:grid-cols-2 @[560px]:grid-cols-3">
          <Meter
            label={t("home.cpu")}
            value={sample ? `${Math.round(sample.cpu)}%` : "—"}
            note={overview ? t("home.threads", { n: overview.cpuThreads }) : ""}
            history={cpuHistory}
            busy={(sample?.cpu ?? 0) >= BUSY}
          />
          <Meter
            label={t("home.memory")}
            value={sample ? formatSize(sample.memoryUsed) : "—"}
            note={sample ? t("home.ofTotal", { total: formatSize(sample.memoryTotal), percent: Math.round(memoryPercent) }) : ""}
            history={memoryHistory}
            busy={memoryPercent >= BUSY}
          />
          <Meter
            label={t("home.gpu")}
            value={gpu ? `${Math.round(gpu.usage)}%` : "—"}
            note={gpuNote}
            title={gpu?.model}
            history={gpuHistory}
            busy={(gpu?.usage ?? 0) >= BUSY}
          />
          <Meter
            label={t("home.network")}
            value={network ? <Rate bytes={network.received} unit={t("home.download")} /> : "—"}
            note={network ? t("home.upload", { rate: rate(network.sent) }) : ""}
            history={downHistory}
            secondary={upHistory}
            scale={RATE_FLOOR}
          />
          <Meter
            label={t("home.diskIo")}
            value={diskIo ? <Rate bytes={diskIo.read} unit={t("home.read")} /> : "—"}
            note={diskIo ? t("home.write", { rate: rate(diskIo.written) }) : ""}
            history={readHistory}
            secondary={writeHistory}
            scale={RATE_FLOOR}
          />
          <Card className="relative overflow-hidden">
            {/* 用掉的部分从左往右铺满整张卡片，在文字后面，和 CPU、内存的走势一样 */}
            {disk && (
              <div
                aria-hidden
                className="pointer-events-none absolute inset-y-0 left-0 border-r"
                style={{
                  width: `${diskPercent}%`,
                  background: `color-mix(in srgb, ${diskPercent >= BUSY ? "var(--warning)" : "var(--accent)"} 10%, transparent)`,
                  borderColor: `color-mix(in srgb, ${diskPercent >= BUSY ? "var(--warning)" : "var(--accent)"} 55%, transparent)`,
                }}
              />
            )}
            <div className="relative">
              <div className="flex items-center text-[12px] text-fg-3">
                {t("home.disk")}
                <button type="button" className="ml-auto flex items-center text-accent hover:underline" onClick={() => useApp.getState().setRoute("disk")}>
                  {t("home.analyzeDisk")}
                  <ChevronRight size={13} strokeWidth={1.75} aria-hidden />
                </button>
              </div>
              <div className={cx("tabular mt-0.5 text-[20px] leading-7 font-semibold", diskPercent >= BUSY && "text-warning")}>
                {disk ? t("home.free", { size: formatSize(disk.available) }) : "—"}
              </div>
              <div className="text-[12px] text-fg-3">{disk ? t("home.ofTotal", { total: formatSize(disk.total), percent: Math.round(diskPercent) }) : ""}</div>
              <div className="h-10" />
            </div>
          </Card>
        </div>

        <div className="mb-3">
          <TopProcesses />
        </div>
        {/* 软件的数量和最近安装左右两栏，窄的时候上下排 */}
        <div className="grid grid-cols-1 gap-3 @[640px]:grid-cols-2">
          <SoftwareStats />
          <RecentInstalls />
        </div>
      </div>
    </div>
  );
}

function Header({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex h-11 shrink-0 items-center gap-2 border-b border-line pr-2 pl-3">
      <h1 className="m-0 min-w-0 truncate text-[13px] font-semibold">{title}</h1>
      {children}
    </div>
  );
}

function Card({ children, className }: { children: ReactNode; className?: string }) {
  return <section className={cx("min-w-0 rounded-lg border border-line px-3 py-2.5", className)}>{children}</section>;
}

function CardTitle({ children, action }: { children: ReactNode; action?: ReactNode }) {
  return (
    <div className="mb-1.5 flex h-6 items-center gap-2 text-[12px] font-semibold text-fg-2">
      {children}
      <span className="ml-auto flex items-center gap-2 font-normal">{action}</span>
    </div>
  );
}

/** 网络、硬盘读写的走势最低按这么多算满格：没什么流量时线贴着底，不会一点点波动就画满整张卡片。 */
const RATE_FLOOR = 512 * 1024;

/** 每秒多少字节：“1.4 MB/s”。 */
function rate(bytes: number): string {
  return `${formatSize(bytes)}/s`;
}

/** 大数字是速度，后面跟一个小字说是下载还是读取。 */
function Rate({ bytes, unit }: { bytes: number; unit: string }) {
  return (
    <>
      {rate(bytes)}
      <span className="ml-1.5 text-[12px] font-normal text-fg-3">{unit}</span>
    </>
  );
}

/**
 * 一张实时卡片：大数字、说明和最近一分钟的走势线。
 * 不传 `scale` 时数值是 0–100 的百分比；传了就是速度，按这一分钟里最大的那个（至少 `scale`）算满格。
 * `secondary` 是第二条线（上传、写入），只画线不填色。
 */
function Meter({
  label,
  value,
  note,
  title,
  history,
  secondary,
  scale,
  busy = false,
}: {
  label: string;
  value: ReactNode;
  note: string;
  title?: string;
  history: number[];
  secondary?: number[];
  scale?: number;
  busy?: boolean;
}) {
  const max = scale === undefined ? 100 : Math.max(scale, ...history, ...(secondary ?? []));
  return (
    <Card className="relative overflow-hidden">
      {/* 走势铺满整张卡片，在文字后面 */}
      <Sparkline values={history} max={max} busy={busy} />
      {secondary && <Sparkline values={secondary} max={max} busy={busy} line />}
      <div className="relative">
        <div className="text-[12px] text-fg-3">{label}</div>
        <div className={cx("tabular mt-0.5 truncate text-[20px] leading-7 font-semibold", busy && "text-warning")}>{value}</div>
        <div className="truncate text-[12px] text-fg-3" title={title}>
          {note || "\u00a0"}
        </div>
        <div className="h-10" />
      </div>
    </Card>
  );
}

/**
 * 走势：0–max，最新的在右边，铺满整张卡片当背景。刚打开、点还不够一分钟时，前面用第一个值补平，不会只在右边画一小截。
 * `line` 时只画一条虚线（第二条线），不填色。
 */
function Sparkline({ values, max = 100, busy, line = false }: { values: number[]; max?: number; busy: boolean; line?: boolean }) {
  const slots = 30;
  if (values.length === 0 || max <= 0) return null;
  const padded = [...Array<number>(Math.max(0, slots - values.length)).fill(values[0]!), ...values].slice(-slots);
  // 最高画到卡片的 85%，上面留点空，满载时也不至于整张都是颜色
  const points = padded.map((v, i) => `${((i / (slots - 1)) * 100).toFixed(2)},${(100 - (Math.min(max, Math.max(0, v)) / max) * 85).toFixed(2)}`);
  const color = busy ? "var(--warning)" : "var(--accent)";
  return (
    <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="pointer-events-none absolute inset-0 h-full w-full" aria-hidden>
      {!line && <polygon points={`0,100 ${points.join(" ")} 100,100`} fill={color} opacity={0.1} />}
      <polyline
        points={points.join(" ")}
        fill="none"
        stroke={color}
        strokeOpacity={line ? 0.45 : 0.55}
        strokeWidth={1.5}
        strokeDasharray={line ? "3 3" : undefined}
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** 进程对应的已安装软件（拿它的图标）：可执行文件在软件的位置里面。 */
function useAppOf(): (process: ProcessInfo) => InstalledApp | null {
  const installed = useApp((state) => state.installed);
  return useMemo(() => {
    const roots = (installed ?? [])
      .filter((app) => app.location)
      .map((app) => ({ app, root: app.location.replace(/[\\/]+$/, "").toLowerCase() }))
      .sort((a, b) => b.root.length - a.root.length);
    return (process) => {
      const exe = process.exe?.toLowerCase();
      if (!exe) return null;
      return roots.find(({ root }) => exe.startsWith(`${root}/`) || exe.startsWith(`${root}\\`))?.app ?? null;
    };
  }, [installed]);
}

/** 进程的图标：后端从它的 .app 或 exe 里取的；没有的话用对应的已安装软件的；都没有用一个终端样子的图标。 */
function ProcessIcon({ process, appOf, size = 16 }: { process: ProcessInfo; appOf: (p: ProcessInfo) => InstalledApp | null; size?: number }) {
  const icon = useSystem((state) => (process.exe ? state.icons[process.exe] : undefined));
  if (icon) return <SoftwareIcon name={process.name} icon={icon} size={size} />;
  const app = appOf(process);
  if (app) return <InstalledSoftwareIcon app={app} size={size} />;
  return (
    <span className="flex shrink-0 items-center justify-center text-fg-4" style={{ width: size, height: size }}>
      <SquareTerminal size={Math.round(size * 0.9)} strokeWidth={1.5} aria-hidden />
    </span>
  );
}

type SortKey = "name" | "pid" | "cpu" | "memory";

function sorted(list: ProcessInfo[], key: SortKey, descending: boolean): ProcessInfo[] {
  const out = [...list].sort((a, b) => {
    const order =
      key === "name" ? a.name.localeCompare(b.name) : key === "pid" ? a.pid - b.pid : key === "cpu" ? a.cpu - b.cpu || a.memory - b.memory : a.memory - b.memory;
    return descending ? -order : order;
  });
  return out;
}

function TopProcesses() {
  const t = useT();
  const sample = useSystem((state) => state.sample);
  const appOf = useAppOf();
  const [by, setBy] = useState<"cpu" | "memory">("cpu");
  const top = sorted(sample?.processes ?? [], by, true).slice(0, 8);
  return (
    <Card>
      <CardTitle
        action={
          <>
            {(["cpu", "memory"] as const).map((key) => (
              <button
                key={key}
                type="button"
                className={cx("rounded px-1.5 text-[12px]", by === key ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2")}
                onClick={() => setBy(key)}
              >
                {t(key === "cpu" ? "home.cpu" : "home.memory")}
              </button>
            ))}
            <button type="button" className="flex items-center text-[12px] text-accent hover:underline" onClick={() => useSystem.setState({ processesOpen: true })}>
              {t("home.allProcesses")}
              <ChevronRight size={13} strokeWidth={1.75} aria-hidden />
            </button>
          </>
        }
      >
        {t("home.processes")}
        {sample && <span className="tabular font-normal text-fg-4">{sample.processes.length}</span>}
      </CardTitle>
      {top.map((process) => (
        <ProcessMenu key={process.pid} process={process}>
          <div className="-mx-1.5 flex h-7 items-center gap-2 rounded-md px-1.5 text-[13px] hover:bg-hover">
            <ProcessIcon process={process} appOf={appOf} />
            <span className="min-w-0 flex-1 truncate" title={process.exe ?? process.name}>
              {process.name}
            </span>
            <span className={cx("tabular w-12 shrink-0 text-right text-[12px]", by === "cpu" ? "text-fg" : "text-fg-3")}>{percent(process.cpu)}</span>
            <span className={cx("tabular w-16 shrink-0 text-right text-[12px]", by === "memory" ? "text-fg" : "text-fg-3")}>{formatSize(process.memory)}</span>
          </div>
        </ProcessMenu>
      ))}
      {!sample && <p className="m-0 py-2 text-[12px] text-fg-3">{t("home.loading")}</p>}
    </Card>
  );
}

/** 软件的数量：每一行点了去对应的页面。 */
function SoftwareStats() {
  const t = useT();
  const library = useApp((state) => state.library);
  const installed = useApp((state) => state.installed);
  const setRoute = useApp((state) => state.setRoute);
  const setScope = useApp((state) => state.setScope);
  const startupCount = useStartup((state) => state.items?.filter((i) => i.kind === "startup" && i.enabled && !i.system).length);
  // 开机自启的数量要先列一遍启动项（只读）
  useEffect(() => {
    const startup = useStartup.getState();
    if (startup.items === null && !startup.loading) void startup.load();
  }, []);
  const cleanupBytes = useCleanup((state) => (state.items && !state.scanning ? state.items.reduce((sum, item) => sum + (item.size ?? 0), 0) : null));

  const software = library?.software ?? [];
  const files = software.flatMap((s) => s.versions.flatMap((v) => v.files));
  const bytes = files.reduce((sum, file) => sum + (file.size ?? 0), 0);
  const updates = software.filter(hasUpdate).length;

  const rows: { label: string; value: string; note?: string; onClick: () => void; highlight?: boolean }[] = [
    { label: t("home.stat.installed"), value: installed ? String(installed.length) : "—", onClick: () => setRoute("installed") },
    {
      label: t("home.stat.library"),
      value: String(software.length),
      note: t("home.stat.files", { n: files.length, size: formatSize(bytes) }),
      onClick: () => setScope({ kind: "all" }),
    },
    { label: t("home.stat.updates"), value: String(updates), onClick: () => setScope({ kind: "updates" }), highlight: updates > 0 },
    { label: t("home.stat.startup"), value: startupCount === undefined ? "—" : String(startupCount), onClick: () => setRoute("startup") },
    { label: t("home.stat.cleanup"), value: cleanupBytes === null ? t("home.stat.notScanned") : formatSize(cleanupBytes), onClick: () => setRoute("cleanup") },
  ];
  return (
    <Card>
      <CardTitle>{t("home.software")}</CardTitle>
      {rows.map((row) => (
        <button
          key={row.label}
          type="button"
          onClick={row.onClick}
          className="-mx-1.5 flex h-7 w-[calc(100%+12px)] items-center gap-2 rounded-md px-1.5 text-left text-[13px] hover:bg-hover"
        >
          <span className="min-w-0 flex-1 truncate">{row.label}</span>
          {row.note && <span className="truncate text-[12px] text-fg-3">{row.note}</span>}
          <span className={cx("tabular shrink-0 text-right font-medium", row.highlight && "text-accent")}>{row.value}</span>
        </button>
      ))}
    </Card>
  );
}

/** 最近装的 6 个软件，点了去已安装里选中它。 */
function RecentInstalls() {
  const t = useT();
  const language = useLanguage();
  const installed = useApp((state) => state.installed);
  const setRoute = useApp((state) => state.setRoute);
  const selectInstalled = useApp((state) => state.selectInstalled);
  const view = usePrefs((state) => state.homeRecentView);
  const setPrefs = usePrefs((state) => state.set);
  const icons = view === "icons";
  const recent = useMemo(
    () => [...(installed ?? [])].filter((app) => app.installedAt).sort((a, b) => b.installedAt.localeCompare(a.installedAt)).slice(0, 8),
    [installed],
  );
  if (recent.length === 0) return null;
  const open = (app: InstalledApp) => {
    setRoute("installed");
    selectInstalled(app.id);
  };
  const toggle = (value: "list" | "icons", label: string, Icon: typeof List) => (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={view === value}
      onClick={() => setPrefs({ homeRecentView: value })}
      className={cx("flex size-6 items-center justify-center rounded", view === value ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2")}
    >
      <Icon size={13} strokeWidth={1.75} aria-hidden />
    </button>
  );
  return (
    <Card>
      <CardTitle
        action={
          <span className="flex items-center gap-0.5">
            {toggle("list", t("home.viewList"), List)}
            {toggle("icons", t("home.viewIcons"), LayoutGrid)}
          </span>
        }
      >
        {t("home.recent")}
      </CardTitle>
      {icons ? (
        <div className="-mx-1 grid grid-cols-[repeat(auto-fill,minmax(76px,1fr))] gap-1">
          {recent.map((app) => (
            <button
              key={app.id}
              type="button"
              title={[app.name, app.version, formatDate(app.installedAt, language)].filter(Boolean).join(" · ")}
              onClick={() => open(app)}
              className="flex min-w-0 flex-col items-center gap-1 rounded-md px-1 pt-2 pb-1.5 hover:bg-hover"
            >
              <InstalledSoftwareIcon app={app} size={32} />
              <span className="w-full truncate text-center text-[12px]">{app.name}</span>
              <span className="tabular -mt-1 text-[12px] text-fg-4">{formatDate(app.installedAt, language)}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className="flex flex-col">
          {recent.slice(0, 6).map((app) => (
            <button
              key={app.id}
              type="button"
              onClick={() => open(app)}
              className="-mx-1.5 flex h-7 min-w-0 items-center gap-2 rounded-md px-1.5 text-left text-[13px] hover:bg-hover"
            >
              <InstalledSoftwareIcon app={app} size={16} />
              <span className="min-w-0 flex-1 truncate">
                {app.name}
                {app.version && <span className="ml-1.5 text-[12px] text-fg-4">{app.version}</span>}
              </span>
              <span className="tabular shrink-0 text-[12px] text-fg-3">{formatDate(app.installedAt, language)}</span>
            </button>
          ))}
        </div>
      )}
    </Card>
  );
}

/** 任务管理器：全部进程，可以搜索、点表头排序，右键结束。 */
function ProcessesPage() {
  const t = useT();
  const sample = useSystem((state) => state.sample);
  const appOf = useAppOf();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; descending: boolean }>({ key: "cpu", descending: true });
  const list = useMemo(() => {
    const words = query.trim().toLowerCase();
    const all = sample?.processes ?? [];
    const found = words ? all.filter((p) => p.name.toLowerCase().includes(words) || String(p.pid) === words || p.exe?.toLowerCase().includes(words)) : all;
    return sorted(found, sort.key, sort.descending);
  }, [sample, query, sort]);

  const column = (key: SortKey, label: string, className: string) => {
    const Arrow = sort.descending ? ArrowDown : ArrowUp;
    return (
      <button
        type="button"
        className={cx("flex items-center gap-1 hover:text-fg-2", className)}
        onClick={() => setSort((current) => ({ key, descending: current.key === key ? !current.descending : key !== "name" }))}
      >
        {label}
        {sort.key === key && <Arrow size={12} strokeWidth={2} aria-hidden />}
      </button>
    );
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <Header
        title={
          <span className="flex items-center gap-1">
            {t("home.processes")}
            {sample && <span className="tabular font-normal text-fg-4">{sample.processes.length}</span>}
          </span>
        }
      >
        <IconButton label={t("home.back")} className="-order-1 -ml-1.5" onClick={() => useSystem.setState({ processesOpen: false })}>
          <ChevronLeft size={16} strokeWidth={1.75} />
        </IconButton>
        <label className="ml-auto flex h-7 w-56 items-center gap-1.5 rounded-md border border-line bg-bg px-2 text-[12px] focus-within:border-accent">
          <Search size={13} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("home.searchProcesses")}
            className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-4"
          />
        </label>
      </Header>
      <div className="grid h-8 shrink-0 grid-cols-[minmax(0,1fr)_64px_64px_88px] items-center gap-2 border-b border-line pr-5 pl-4 text-[12px] text-fg-3">
        {column("name", t("home.col.name"), "")}
        {column("pid", "PID", "justify-end")}
        {column("cpu", t("home.cpu"), "justify-end")}
        {column("memory", t("home.memory"), "justify-end")}
      </div>
      <div className="min-h-0 flex-1 overflow-y-scroll px-2 pt-1 pb-16 [scrollbar-gutter:stable]">
        {list.map((process) => (
          <ProcessMenu key={process.pid} process={process}>
            <div className="grid h-8 grid-cols-[minmax(0,1fr)_64px_64px_88px] items-center gap-2 rounded-md px-2 text-[13px] hover:bg-hover">
              <span className="flex min-w-0 items-center gap-2">
                <ProcessIcon process={process} appOf={appOf} />
                <span className={cx("truncate", process.protected && "text-fg-2")} title={process.exe ?? process.name}>
                  {process.name}
                </span>
              </span>
              <span className="tabular text-right text-[12px] text-fg-3">{process.pid}</span>
              <span className="tabular text-right text-[12px]">{percent(process.cpu)}</span>
              <span className="tabular text-right text-[12px]">{formatSize(process.memory)}</span>
            </div>
          </ProcessMenu>
        ))}
        {sample && list.length === 0 && <p className="m-0 px-2 py-4 text-center text-[12px] text-fg-3">{t("home.noProcesses")}</p>}
      </div>
    </div>
  );
}

/** 进程的右键菜单：结束进程、强制结束。系统进程、别的用户的、ShibaPack 自己的不能结束。 */
function ProcessMenu({ process, children }: { process: ProcessInfo; children: ReactNode }) {
  const t = useT();
  const language = useLanguage();
  const notify = useApp((state) => state.notify);
  const refresh = useSystem((state) => state.refresh);

  const end = async (force: boolean) => {
    const { ok } = await confirm({
      title: t(force ? "home.forceEndTitle" : "home.endTitle", { name: process.name }),
      body: t(force ? "home.forceEndBody" : "home.endBody"),
      confirmLabel: t(force ? "home.forceEnd" : "home.end"),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.endProcess(process.pid, force);
    } catch (reason) {
      notify(errorText(language, reason), "error");
      return;
    }
    if (force) {
      await refresh();
      return;
    }
    // 给它几秒正常退出；还在的话问要不要强制结束
    await new Promise((resolve) => setTimeout(resolve, 3000));
    await refresh();
    if (useSystem.getState().sample?.processes.some((p) => p.pid === process.pid)) {
      const again = await confirm({
        title: t("home.stillRunning", { name: process.name }),
        body: t("home.forceEndBody"),
        confirmLabel: t("home.forceEnd"),
        danger: true,
      });
      if (again.ok) {
        try {
          await api.endProcess(process.pid, true);
          await refresh();
        } catch (reason) {
          notify(errorText(language, reason), "error");
        }
      }
    }
  };

  const items: MenuEntry[] = process.protected
    ? [{ label: t("home.protected"), icon: CircleX, disabled: true, onSelect: () => {} }]
    : [
        { label: t("home.end"), icon: CircleX, onSelect: () => void end(false) },
        { label: t("home.forceEnd"), icon: Skull, danger: true, onSelect: () => void end(true) },
      ];
  return (
    <ContextMenu items={items} onOpenChange={(open) => useSystem.setState({ paused: open })}>
      {children}
    </ContextMenu>
  );
}

/** 本机信息：型号、系统、处理器、显卡、内存、磁盘、开机时间。可以复制成文字。 */
function HardwareDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const language = useLanguage();
  const notify = useApp((state) => state.notify);
  const overview = useSystem((state) => state.overview)!;
  const sample = useSystem((state) => state.sample);
  const cores = overview.coreSplit
    ? t("home.hw.coreSplit", { n: overview.cpuCores ?? overview.cpuThreads, p: overview.coreSplit[0], e: overview.coreSplit[1] })
    : t("home.cores", { n: overview.cpuCores ?? overview.cpuThreads });
  const rows: [string, string[]][] = [
    [t("home.hw.name"), [overview.hostName]],
    [t("home.hw.model"), [overview.model]],
    [t("home.hw.os"), [[overview.os, overview.kernel && t("home.hw.kernel", { v: overview.kernel })].filter(Boolean).join(" · ")]],
    [t("home.hw.arch"), [overview.arch]],
    [t("home.cpu"), [overview.cpuBrand, [cores, t("home.threads", { n: overview.cpuThreads }), overview.cpuMhz > 0 ? `${(overview.cpuMhz / 1000).toFixed(2)} GHz` : ""].filter(Boolean).join(" · ")]],
    [t("home.hw.gpu"), overview.gpus.map((gpu) => [gpu.model, gpu.memory ? formatSize(gpu.memory) : ""].filter(Boolean).join(" · "))],
    [t("home.memory"), [[formatSize(overview.totalMemory), overview.totalSwap > 0 ? t("home.hw.swap", { size: formatSize(overview.totalSwap) }) : ""].filter(Boolean).join(" · ")]],
    [
      t("home.hw.disks"),
      overview.disks.map((disk) =>
        [
          disk.name && disk.name !== disk.mount ? `${disk.name} (${disk.mount})` : disk.mount,
          disk.kind,
          t("home.hw.diskSpace", { free: formatSize(disk.available), total: formatSize(disk.total) }),
          disk.removable ? t("home.hw.removable") : "",
        ]
          .filter(Boolean)
          .join(" · "),
      ),
    ],
    [
      t("home.hw.boot"),
      [[overview.bootTime ? new Date(overview.bootTime * 1000).toLocaleString(language) : "", sample ? t("home.uptime", { time: duration(sample.uptime, t) }) : ""].filter(Boolean).join(" · ")],
    ],
  ];
  const shown = rows.map(([label, values]) => [label, values.filter(Boolean)] as const).filter(([, values]) => values.length > 0);
  const copy = async () => {
    const text = shown.map(([label, values]) => `${label}: ${values.join(" / ")}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      notify(t("home.hw.copied"));
    } catch {
      notify(t("home.hw.copyFailed"), "error");
    }
  };
  return (
    <Modal
      open
      onClose={onClose}
      title={t("home.hardware")}
      width={560}
      footer={
        <>
          <Button variant="ghost" className="mr-auto" onClick={() => void copy()}>
            <Copy size={14} strokeWidth={1.75} aria-hidden />
            {t("home.hw.copy")}
          </Button>
          <Button variant="primary" onClick={onClose}>
            {t("common.close")}
          </Button>
        </>
      }
    >
      <dl className="m-0">
        {shown.map(([label, values]) => (
          <div key={label} className="flex items-start gap-3 border-b border-line py-2 text-[13px] last:border-b-0">
            <dt className="w-[72px] shrink-0 text-[12px] leading-[19px] text-fg-3">{label}</dt>
            <dd className="selectable m-0 min-w-0 flex-1">
              {values.map((value) => (
                <div key={value} className="break-words">
                  {value}
                </div>
              ))}
            </dd>
          </div>
        ))}
      </dl>
    </Modal>
  );
}

function percent(value: number): string {
  return value >= 10 ? `${Math.round(value)}%` : `${value.toFixed(1)}%`;
}

/** 已运行多久：“3 天 4 小时”“5 小时 12 分钟”。 */
function duration(seconds: number, t: ReturnType<typeof useT>): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return t("home.daysHours", { d: days, h: hours });
  if (hours > 0) return t("home.hoursMinutes", { h: hours, m: minutes });
  return t("home.minutes", { m: minutes });
}
