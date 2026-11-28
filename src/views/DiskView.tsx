// 磁盘分析：选一个文件夹或一块盘扫一遍，用扇形图（一圈是一层）或矩形图（面积是大小）看空间都去哪了。
// 点文件夹往里走，点中间（或上面的路径）往回走；右键色块可以打开、在文件夹中显示、复制路径、移到废纸篓。
//
// 扫完的树在后端内存里（lib/disk.ts 只拿要画的几层），什么都不存。

import {
  ChartPie,
  ChevronDown,
  ChevronRight,
  Copy,
  Disc,
  ExternalLink,
  File,
  FileArchive,
  FileAudio,
  FileImage,
  FileVideo,
  Folder,
  FolderOpen,
  FolderSearch,
  HardDrive,
  House,
  LayoutDashboard,
  List,
  LoaderCircle,
  Package,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ResizeHandle } from "../components/chrome";
import { confirm } from "../components/dialogs";
import { Button, ContextMenu, Dropdown, IconButton, IconSegmented, type MenuEntry, cx } from "../components/ui";
import { api, isMac, pickFolder } from "../lib/api";
import { explainable, useDisk, VIEW_DEPTH } from "../lib/disk";
import { type Key, errorText } from "../lib/i18n";
import { formatDate, formatSize } from "../lib/library";
import { DISK_LIST, usePrefs } from "../lib/prefs";
import { useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { DiskNode, LargeFile } from "../lib/types";

export function DiskView() {
  const t = useT();
  const language = useLanguage();
  const chart = usePrefs((state) => state.diskChart);
  const setPrefs = usePrefs((state) => state.set);
  const { locations, root, scanning, summary, view, error, loadLocations, scan, cancel } = useDisk();

  useEffect(() => {
    if (locations === null) void loadLocations();
  }, [locations, loadLocations]);

  const current = locations?.find((l) => l.path === root);
  const chooseFolder = async () => {
    const folder = await pickFolder();
    if (folder) void scan(folder);
  };
  const places: MenuEntry[] = [
    ...(locations ?? []).map((location) => ({
      label: location.kind === "home" ? t("disk.home") : `${location.name} (${location.path})`,
      icon: location.kind === "home" ? House : HardDrive,
      checked: location.path === root,
      onSelect: () => void scan(location.path),
    })),
    { label: t("disk.chooseFolder"), icon: FolderSearch, separated: true, onSelect: () => void chooseFolder() },
  ];
  const rootLabel = current ? (current.kind === "home" ? t("disk.home") : current.name) : (root ?? t("disk.choose"));

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-2 border-b border-line pr-2 pl-3">
        <h1 className="m-0 shrink-0 text-[13px] font-semibold">{t("nav.disk")}</h1>
        <Dropdown items={places} align="start">
          <button
            type="button"
            disabled={scanning}
            className="flex h-7 min-w-0 items-center gap-1 rounded-md px-2 text-[12px] text-fg-2 hover:bg-hover disabled:opacity-50"
          >
            <span className="truncate">{rootLabel}</span>
            <ChevronDown size={13} strokeWidth={1.75} aria-hidden className="shrink-0 opacity-60" />
          </button>
        </Dropdown>
        <span className="flex-1" />
        {view && (summary || scanning) && (
          <IconSegmented
            value={chart}
            onChange={(diskChart) => setPrefs({ diskChart })}
            options={[
              { value: "sunburst", label: t("disk.sunburst"), icon: <ChartPie size={14} strokeWidth={1.75} /> },
              { value: "treemap", label: t("disk.treemap"), icon: <LayoutDashboard size={14} strokeWidth={1.75} /> },
              { value: "list", label: t("disk.list"), icon: <List size={14} strokeWidth={1.75} /> },
            ]}
          />
        )}
        {scanning ? (
          <IconButton label={t("disk.stop")} onClick={cancel}>
            <X size={15} strokeWidth={1.75} />
          </IconButton>
        ) : (
          summary && (
            <IconButton label={t("disk.rescan")} onClick={() => void scan()}>
              <RefreshCw size={15} strokeWidth={1.75} />
            </IconButton>
          )
        )}
      </div>

      {error !== null && error !== undefined && !scanning && (
        <p className="m-0 mx-3 mt-2 rounded-lg bg-[color-mix(in_srgb,var(--danger)_12%,transparent)] px-3 py-2 text-[12px] text-danger">{errorText(language, error)}</p>
      )}

      {/* 扫描中一有结果就先画出来（边扫边看），还没有时显示进度 */}
      {view && (summary || scanning) ? <Result chart={chart} /> : scanning ? <Scanning /> : <Start onChooseFolder={() => void chooseFolder()} />}
    </div>
  );
}

/** 还没扫：列出用户文件夹和每一块盘，点一个开始。 */
function Start({ onChooseFolder }: { onChooseFolder: () => void }) {
  const t = useT();
  const { locations, scan } = useDisk();
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 overflow-y-auto p-6">
      <div className="text-center">
        <ChartPie size={30} strokeWidth={1.5} aria-hidden className="mx-auto text-fg-4" />
        <p className="m-0 mt-2 text-[14px] font-semibold">{t("disk.startTitle")}</p>
        <p className="m-0 mt-1 text-[12px] text-fg-3">{t("disk.startHint")}</p>
      </div>
      <div className="flex w-full max-w-[460px] flex-col gap-1.5">
        {(locations ?? []).map((location) => {
          const used = location.total > 0 ? (location.total - location.available) / location.total : 0;
          return (
            <button
              key={location.path}
              type="button"
              onClick={() => void scan(location.path)}
              className="flex items-center gap-3 rounded-lg border border-line px-3 py-2.5 text-left transition-colors hover:bg-hover"
            >
              {location.kind === "home" ? (
                <House size={18} strokeWidth={1.5} aria-hidden className="shrink-0 text-fg-3" />
              ) : (
                <HardDrive size={18} strokeWidth={1.5} aria-hidden className="shrink-0 text-fg-3" />
              )}
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13px] font-medium">{location.kind === "home" ? t("disk.home") : location.name}</span>
                <span className="block truncate text-[12px] text-fg-3">
                  {location.total > 0 ? t("disk.space", { free: formatSize(location.available), total: formatSize(location.total) }) : location.path}
                </span>
                {location.total > 0 && (
                  <span className="mt-1.5 block h-1 overflow-hidden rounded-full bg-hover">
                    <span className="block h-full rounded-full bg-accent-fill" style={{ width: `${used * 100}%` }} />
                  </span>
                )}
              </span>
              <ChevronRight size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-4" />
            </button>
          );
        })}
        <Button variant="ghost" className="mt-1 self-center" onClick={onChooseFolder}>
          <FolderSearch size={14} strokeWidth={1.75} aria-hidden />
          {t("disk.chooseFolder")}
        </Button>
      </div>
    </div>
  );
}

function Scanning() {
  const t = useT();
  const progress = useDisk((state) => state.progress);
  const cancel = useDisk((state) => state.cancel);
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-6">
      <LoaderCircle size={26} strokeWidth={1.75} aria-hidden className="animate-spin text-accent" />
      <p className="tabular m-0 text-[13px] font-medium">
        {progress ? t("disk.scanned", { files: progress.files.toLocaleString(), size: formatSize(progress.bytes) }) : t("disk.starting")}
      </p>
      <p className="m-0 h-4 w-full max-w-[520px] truncate text-center font-mono text-[12px] text-fg-3">{progress?.current ?? ""}</p>
      <Button variant="ghost" onClick={cancel}>
        {t("disk.stop")}
      </Button>
    </div>
  );
}

/** 扫完（或者正在扫、先看着）：上面是路径，左边是图，右边是这一层的列表。 */
function Result({ chart }: { chart: "sunburst" | "treemap" | "list" }) {
  const t = useT();
  const { summary, view, path, open, root, scanning, progress } = useDisk();
  const [tip, setTip] = useState<{ node: DiskNode; x: number; y: number } | null>(null);
  const [target, setTarget] = useState<DiskNode | null>(null);
  const colors = useColors(view);
  // 扫描中树还没拼好，不能删也不能定位到更深的地方，先不给右键菜单
  const nodeMenu = useNodeMenu(target ?? view);
  const menu = scanning ? [] : nodeMenu;
  const listWidth = usePrefs((state) => state.diskListWidth);
  const setPrefs = usePrefs((state) => state.set);
  const aiReady = useAiReady() && !scanning;
  const lookUp = useDisk((state) => state.lookUp);
  const ask = useDisk((state) => state.ask);
  // 打开一层时先把缓存里有的解释拿出来，鼠标停上去马上就有
  useEffect(() => {
    if (aiReady && view) void lookUp([view, ...view.children]);
  }, [aiReady, view, lookUp]);
  // 鼠标在同一项上停够 0.6 秒才问 AI，扫过去不算
  const hovering = tip?.node ?? null;
  useEffect(() => {
    if (!aiReady || !hovering || !explainable(hovering)) return;
    const timer = setTimeout(() => void ask(hovering), 600);
    return () => clearTimeout(timer);
  }, [aiReady, hovering, ask]);
  if (!view || !(summary || scanning)) return null;

  const rootPath = summary?.root ?? root ?? "";
  const rootName = rootPath.split(/[\\/]/).filter(Boolean).pop() ?? rootPath;
  const handlers: ChartHandlers = {
    colors,
    onHover: (node, event) => setTip(node && event ? { node, x: event.clientX, y: event.clientY } : null),
    onOpen: (node) => {
      if (node.hasChildren) void open(node.path);
    },
    onTarget: setTarget,
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-line px-3 text-[12px]">
        <nav className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
          {[rootName, ...path].map((segment, i) => (
            <span key={i} className="flex min-w-0 items-center gap-0.5">
              {i > 0 && <ChevronRight size={12} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-4" />}
              <button
                type="button"
                onClick={() => void open(path.slice(0, i))}
                className={cx("truncate rounded px-1 py-0.5 hover:bg-hover", i === path.length ? "font-semibold text-fg" : "text-fg-3")}
              >
                {segment}
              </button>
            </span>
          ))}
        </nav>
        {scanning && (
          <span className="tabular flex shrink-0 items-center gap-1.5 text-fg-3" title={t("disk.liveHint")}>
            <LoaderCircle size={12} strokeWidth={2} aria-hidden className="animate-spin text-accent" />
            {t("disk.live", { files: (progress?.files ?? 0).toLocaleString(), size: formatSize(progress?.bytes ?? 0) })}
          </span>
        )}
      </div>
      {/* 扫描中：一条细的进度条在走 */}
      {scanning && (
        <div className="relative h-0.5 shrink-0 overflow-hidden bg-hover">
          <div className="absolute inset-y-0 w-1/3 animate-slide rounded-full bg-accent-fill" />
        </div>
      )}
      <ContextMenu items={menu}>
        {/* 右键空白处就是现在这一层；点到色块或列表里的一项时，那一项先把自己记成目标 */}
        <div className="@container flex min-h-0 flex-1" onContextMenuCapture={() => setTarget(null)}>
          {chart === "list" ? (
            <LargestFiles view={view} title={[rootName, ...path].pop() ?? rootName} onTarget={setTarget} />
          ) : (
            <>
              <div className="relative min-w-0 flex-1" onMouseLeave={() => setTip(null)}>
                {chart === "sunburst" ? <Sunburst view={view} {...handlers} /> : <Treemap view={view} {...handlers} />}
              </div>
              {/* 中间的分隔线拖动调右边列表的宽度，双击回到默认 */}
              <div className="hidden @[600px]:flex">
                <ResizeHandle edge="right" width={listWidth} onResize={(width) => setPrefs({ diskListWidth: width })} onReset={() => setPrefs({ diskListWidth: DISK_LIST.initial })} />
              </div>
              <ChildList view={view} width={listWidth} {...handlers} />
            </>
          )}
        </div>
      </ContextMenu>
      {tip && <Tooltip node={tip.node} x={tip.x} y={tip.y} total={view.size} ai={aiReady} />}
    </div>
  );
}

interface ChartHandlers {
  colors: Map<string, string>;
  onHover: (node: DiskNode | null, event?: { clientX: number; clientY: number }) => void;
  onOpen: (node: DiskNode) => void;
  /** 右键之前记下点的是哪一项，菜单按它来。 */
  onTarget: (node: DiskNode | null) => void;
}

/** 一项的名字：“小文件”“其他 N 项”有自己的说法。 */
function useLabel(): (node: DiskNode) => string {
  const t = useT();
  return (node) => (node.kind === "small" ? t("disk.smallFiles") : node.kind === "other" ? t("disk.other", { n: node.name }) : node.name);
}

/** 第一层每一项一个颜色，往下同一个色相、越深越浅；小文件、其他是灰色。 */
const HUES = [214, 158, 28, 344, 266, 190, 6, 96, 300, 48];
const GRAY = "hsl(220 8% 56%)";

/** 每一项的“底色”：第一层每项一个色相，下面的跟着它。真正画的时候用 `tint` 按深浅和背景混，浅色、深色主题都好看。 */
function useColors(view: DiskNode | null): Map<string, string> {
  return useMemo(() => {
    const out = new Map<string, string>();
    if (!view) return out;
    const paint = (node: DiskNode, base: string, parent: string[]) => {
      out.set(keyOf(node, parent), node.kind === "small" || node.kind === "other" ? GRAY : base);
      node.children.forEach((child) => paint(child, base, node.path));
    };
    view.children.forEach((child, i) => paint(child, `hsl(${HUES[i % HUES.length]} 64% 52%)`, view.path));
    return out;
  }, [view]);
}

/** 底色和页面背景按比例混：比例越小越浅（深色主题下越暗）。 */
function tint(color: string | undefined, percent: number): string {
  return `color-mix(in srgb, ${color ?? GRAY} ${percent}%, var(--bg))`;
}

/** 鼠标停在某一项上（图表或右边的列表），另一边跟着高亮。没停在任何一项上时是 null。 */
const setHoverKey = (key: string | null) => {
  if (useDisk.getState().hover !== key) useDisk.setState({ hover: key });
};

/** `key` 这一项要不要高亮：就是停着的那项，或者在它里面（停在列表的文件夹上时，扇形图里它外圈的子项也亮）。 */
function lit(key: string, hover: string | null): boolean {
  return hover !== null && (key === hover || key.startsWith(`${hover}/`));
}

function keyOf(node: DiskNode, parent: string[]): string {
  return node.path.length > 0 ? node.path.join("/") : `${parent.join("/")}/:${node.kind}`;
}

function useSize(ref: React.RefObject<HTMLDivElement | null>): { width: number; height: number } {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return size;
}

// ── 扇形图 ─────────────────────────────────────────────────

interface Arc {
  node: DiskNode;
  key: string;
  depth: number;
  start: number;
  end: number;
}

function Sunburst({ view, colors, onHover, onOpen, onTarget }: { view: DiskNode } & ChartHandlers) {
  const t = useT();
  const label = useLabel();
  const frame = useRef<HTMLDivElement>(null);
  const { width, height } = useSize(frame);
  const path = useDisk((state) => state.path);
  const open = useDisk((state) => state.open);
  const hover = useDisk((state) => state.hover);

  const arcs = useMemo(() => {
    const out: Arc[] = [];
    const place = (node: DiskNode, depth: number, start: number, end: number) => {
      if (node.size <= 0) return;
      let angle = start;
      for (const child of node.children) {
        const span = ((end - start) * child.size) / node.size;
        if (span > 0.008) out.push({ node: child, key: keyOf(child, node.path), depth, start: angle, end: angle + span });
        if (span > 0.008 && depth < VIEW_DEPTH) place(child, depth + 1, angle, angle + span);
        angle += span;
      }
    };
    place(view, 1, 0, Math.PI * 2);
    return out;
  }, [view]);

  const size = Math.max(0, Math.min(width, height) - 24);
  const outer = size / 2;
  const inner = outer * 0.3;
  const ring = (outer - inner) / VIEW_DEPTH;
  const hovered = arcs.find((arc) => arc.key === hover)?.node ?? null;
  const center = hovered ?? view;

  return (
    <div ref={frame} className="absolute inset-0 flex items-center justify-center">
      {size > 40 && (
        <svg width={size} height={size} viewBox={`${-outer} ${-outer} ${size} ${size}`} role="img" aria-label={t("disk.sunburst")}>
          {arcs.map((arc) => (
            <path
              key={arc.key}
              d={annulus(inner + (arc.depth - 1) * ring, inner + arc.depth * ring - 1, arc.start, arc.end)}
              fill={tint(colors.get(arc.key), [86, 70, 56, 44][arc.depth - 1] ?? 40)}
              stroke="var(--bg)"
              strokeWidth={1}
              className={cx(
                "transition-opacity",
                hover !== null && !lit(arc.key, hover) && "opacity-60",
                hover === null && arc.node.pending && "opacity-75",
                arc.node.hasChildren && "cursor-pointer",
              )}
              onMouseMove={(event) => {
                setHoverKey(arc.key);
                onHover(arc.node, event);
              }}
              onMouseLeave={() => {
                setHoverKey(null);
                onHover(null);
              }}
              onClick={() => onOpen(arc.node)}
              onContextMenu={() => onTarget(arc.node)}
            />
          ))}
          {/* 中间：现在这一层（或鼠标停着的那项）。点了往上一层 */}
          <circle
            r={inner - 2}
            fill="var(--bg)"
            className={cx(path.length > 0 && "cursor-pointer")}
            onClick={() => path.length > 0 && void open(path.slice(0, -1))}
            onContextMenu={() => onTarget(view)}
          />
          <text textAnchor="middle" y={-6} className="pointer-events-none fill-[var(--fg)] text-[13px] font-semibold">
            {truncate(label(center), Math.max(6, Math.floor(inner / 7)))}
          </text>
          <text textAnchor="middle" y={13} className="tabular pointer-events-none fill-[var(--fg-3)] text-[12px]">
            {formatSize(center.size)}
          </text>
          {path.length > 0 && !hovered && (
            <text textAnchor="middle" y={31} className="pointer-events-none fill-[var(--fg-4)] text-[12px]">
              {t("disk.up")}
            </text>
          )}
        </svg>
      )}
    </div>
  );
}

/** 圆环上的一段：从 start 到 end（弧度，从 12 点钟方向顺时针）。 */
function annulus(r0: number, r1: number, start: number, end: number): string {
  // 一整圈画不出来（起点终点重合），差一点点
  const sweep = Math.min(end - start, Math.PI * 2 - 0.0001);
  const a0 = start - Math.PI / 2;
  const a1 = a0 + sweep;
  const large = sweep > Math.PI ? 1 : 0;
  const point = (r: number, a: number) => `${(r * Math.cos(a)).toFixed(2)} ${(r * Math.sin(a)).toFixed(2)}`;
  return [
    `M ${point(r1, a0)}`,
    `A ${r1} ${r1} 0 ${large} 1 ${point(r1, a1)}`,
    `L ${point(r0, a1)}`,
    `A ${r0} ${r0} 0 ${large} 0 ${point(r0, a0)}`,
    "Z",
  ].join(" ");
}

function truncate(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

// ── 矩形图 ─────────────────────────────────────────────────

interface Box {
  node: DiskNode;
  key: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 色块之间的缝（带圆角，缝稍宽一点才看得出一块一块）。 */
const GAP = 2;

function Treemap({ view, colors, onHover, onOpen, onTarget }: { view: DiskNode } & ChartHandlers) {
  const label = useLabel();
  const frame = useRef<HTMLDivElement>(null);
  const { width, height } = useSize(frame);
  const hover = useDisk((state) => state.hover);

  // 只画当前文件夹里的一层，不在方块里再套子方块；点文件夹进去看下一层
  const boxes = useMemo((): Box[] => {
    if (width <= 0 || height <= 0) return [];
    const items = view.children.filter((c) => c.size > 0);
    return squarify(items, 0, 0, width, height).map((cell) => ({ node: cell.item, key: keyOf(cell.item, view.path), x: cell.x, y: cell.y, w: cell.w, h: cell.h }));
  }, [view, width, height]);

  return (
    <div ref={frame} className="absolute inset-3 overflow-hidden">
      {boxes.map((box) => {
        const base = colors.get(box.key);
        const muted = box.node.kind === "small" || box.node.kind === "other";
        // 字是白的，底色要够深：文件比文件夹浅一点，“其他”和零碎的更浅
        const fill = tint(base, muted ? 55 : box.node.kind === "file" ? 82 : 94);
        const w = box.w - GAP;
        const h = box.h - GAP;
        const name = w > 48 && h > 22;
        const size = w > 56 && h > 42;
        // 文件夹在名字左边放一个小文件夹图标，地方够才放
        const icon = box.node.kind === "dir" && w > 64;
        return (
          <div
            key={box.key}
            className={cx(
              // 停着的那块（图上或右边列表里）整块提亮一点，不加轮廓
              "absolute overflow-hidden rounded-md transition-[filter] duration-150",
              lit(box.key, hover) && "brightness-110 saturate-[1.15]",
              box.node.hasChildren ? "cursor-pointer" : "cursor-default",
            )}
            style={{
              left: box.x,
              top: box.y,
              width: w,
              height: h,
              background: fill,
              // 还没扫完的：斜纹，大小还会变
              backgroundImage: box.node.pending ? "repeating-linear-gradient(135deg, rgb(255 255 255 / 0.14) 0 6px, transparent 6px 12px)" : undefined,
            }}
            onMouseMove={(event) => {
              event.stopPropagation();
              setHoverKey(box.key);
              onHover(box.node, event);
            }}
            onMouseLeave={() => {
              setHoverKey(null);
              onHover(null);
            }}
            onClick={(event) => {
              event.stopPropagation();
              onOpen(box.node);
            }}
            onContextMenu={() => onTarget(box.node)}
          >
            {name && (
              <div className="pointer-events-none flex h-full flex-col items-center justify-center px-2 text-center text-[12px] leading-[16px] text-white [text-shadow:0_1px_2px_rgb(0_0_0/0.25)]">
                <div className="flex max-w-full items-center gap-1">
                  {icon && <Folder size={13} strokeWidth={2} aria-hidden className="shrink-0 drop-shadow-[0_1px_2px_rgb(0_0_0/0.25)]" />}
                  <span className="min-w-0 truncate font-medium">{label(box.node)}</span>
                </div>
                {size && (
                  <div className="tabular max-w-full truncate text-white/85">
                    {formatSize(box.node.size)}
                    {box.node.pending && "…"}
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** 方块尽量接近正方形的排法（squarified treemap）。 */
function squarify<T extends { size: number }>(items: T[], x: number, y: number, w: number, h: number): { item: T; x: number; y: number; w: number; h: number }[] {
  const out: { item: T; x: number; y: number; w: number; h: number }[] = [];
  const total = items.reduce((sum, item) => sum + item.size, 0);
  if (total <= 0 || w <= 1 || h <= 1) return out;
  const scale = (w * h) / total;
  let rest = items.map((item) => ({ item, area: item.size * scale }));
  let box = { x, y, w, h };
  while (rest.length > 0) {
    const short = Math.min(box.w, box.h);
    let row: typeof rest = [];
    let best = Infinity;
    for (const cell of rest) {
      const next = [...row, cell];
      const worst = worstRatio(next, short);
      if (worst > best) break;
      row = next;
      best = worst;
    }
    const area = row.reduce((sum, cell) => sum + cell.area, 0);
    const thick = area / short;
    let offset = 0;
    for (const cell of row) {
      const length = cell.area / thick;
      if (box.w >= box.h) out.push({ item: cell.item, x: box.x, y: box.y + offset, w: thick, h: length });
      else out.push({ item: cell.item, x: box.x + offset, y: box.y, w: length, h: thick });
      offset += length;
    }
    box = box.w >= box.h ? { x: box.x + thick, y: box.y, w: box.w - thick, h: box.h } : { x: box.x, y: box.y + thick, w: box.w, h: box.h - thick };
    rest = rest.slice(row.length);
  }
  return out;
}

function worstRatio(row: { area: number }[], short: number): number {
  const sum = row.reduce((total, cell) => total + cell.area, 0);
  const max = Math.max(...row.map((cell) => cell.area));
  const min = Math.min(...row.map((cell) => cell.area));
  return Math.max((short * short * max) / (sum * sum), (sum * sum) / (short * short * min));
}

// ── 列表、提示、右键菜单 ────────────────────────────────────

/** 右边：这一层的每一项，颜色和图上对得上。 */
function ChildList({ view, width, colors, onOpen, onTarget }: { view: DiskNode; width: number } & ChartHandlers) {
  const t = useT();
  const label = useLabel();
  const hover = useDisk((state) => state.hover);
  return (
    <div className="hidden shrink-0 flex-col overflow-y-auto p-1.5 @[600px]:flex" style={{ width }}>
      {view.children.map((child) => {
        const key = keyOf(child, view.path);
        const share = view.size > 0 ? child.size / view.size : 0;
        return (
            <button
              key={key}
              type="button"
              onClick={() => onOpen(child)}
              onContextMenu={() => onTarget(child)}
              onMouseEnter={() => setHoverKey(key)}
              onMouseLeave={() => setHoverKey(null)}
              className={cx("flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left", hover !== null && (hover === key || hover.startsWith(`${key}/`)) && "bg-hover", !child.hasChildren && "cursor-default")}
            >
              <span className="size-2.5 shrink-0" style={{ background: tint(colors.get(key), 86) }} />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-[13px]">{label(child)}</span>
                  <span className="tabular shrink-0 text-[12px] text-fg-3" title={child.pending ? t("disk.pending") : undefined}>
                    {formatSize(child.size)}
                    {child.pending && "…"}
                  </span>
                </span>
                <span className="mt-1 block h-[3px] overflow-hidden rounded-full bg-hover">
                  <span className={cx("block h-full rounded-full", child.pending && "animate-pulse")} style={{ width: `${share * 100}%`, background: tint(colors.get(key), 86) }} />
                </span>
              </span>
            </button>
        );
      })}
    </div>
  );
}

// ── 最大的文件 ─────────────────────────────────────────────────

/** 一次列多少个。 */
const LARGEST = 200;

type FileType = "video" | "audio" | "image" | "archive" | "installer" | "diskImage" | "vm" | "other";

const EXTENSIONS: Record<Exclude<FileType, "other">, string[]> = {
  video: ["mp4", "mov", "mkv", "avi", "m4v", "webm", "wmv", "flv", "mts"],
  audio: ["mp3", "m4a", "wav", "flac", "aac", "aiff", "ogg"],
  image: ["jpg", "jpeg", "png", "heic", "cr2", "nef", "arw", "psd", "tif", "tiff", "dng"],
  archive: ["zip", "rar", "7z", "tar", "gz", "tgz", "xz", "bz2", "zst"],
  installer: ["pkg", "mpkg", "exe", "msi", "msix", "appx", "deb", "rpm", "appimage", "apk", "xip"],
  diskImage: ["dmg", "iso", "img", "sparseimage", "sparsebundle"],
  // .raw 多半是虚拟机磁盘（Docker 的 Docker.raw）
  vm: ["vmdk", "vdi", "vhd", "vhdx", "qcow2", "hdd", "raw", "utm", "ova"],
};

function fileType(name: string): FileType {
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
  for (const [type, list] of Object.entries(EXTENSIONS)) if (list.includes(ext)) return type as FileType;
  return "other";
}

const TYPE_ICON: Record<FileType, typeof File> = {
  video: FileVideo,
  audio: FileAudio,
  image: FileImage,
  archive: FileArchive,
  installer: Package,
  diskImage: Disc,
  vm: HardDrive,
  other: File,
};

/** 当前文件夹（连同下面所有层）里最大的文件，不分层级排在一起。右键和图上的一样：打开、定位、拷贝路径、移到废纸篓。 */
function LargestFiles({ view, title, onTarget }: { view: DiskNode; title: string; onTarget: (node: DiskNode | null) => void }) {
  const t = useT();
  const language = useLanguage();
  const [files, setFiles] = useState<LargeFile[] | null>(null);
  const [error, setError] = useState<unknown>(null);

  const scanning = useDisk((state) => state.scanning);
  // 打开别的文件夹、删了东西以后（view 换了）重新取；扫描中 view 每次刷新都会换，跟着取扫到现在最大的
  useEffect(() => {
    let alive = true;
    setError(null);
    const prefix = view.path.join("/");
    const load = scanning
      ? api.diskLiveLargest().then((list) => list.filter((f) => !prefix || f.path.join("/").startsWith(`${prefix}/`)))
      : api.diskLargest(view.path, LARGEST);
    load.then(
      (list) => alive && setFiles(list),
      (reason) => alive && setError(reason),
    );
    return () => {
      alive = false;
    };
  }, [view, scanning]);

  if (error) return <p className="m-0 p-4 text-[12px] text-danger">{errorText(language, error)}</p>;
  if (!files) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-[12px] text-fg-3">
        <LoaderCircle size={14} strokeWidth={1.75} className="animate-spin" aria-hidden />
        {t("disk.listLoading")}
      </div>
    );
  }
  if (files.length === 0) return <p className="m-0 flex flex-1 items-center justify-center text-[13px] text-fg-3">{t("disk.listEmpty")}</p>;

  const biggest = files[0]?.size ?? 1;
  const total = files.reduce((sum, f) => sum + f.size, 0);
  const asNode = (file: LargeFile): DiskNode => ({ name: file.name, path: file.path, kind: "file", size: file.size, files: 1, hasChildren: false, children: [] });

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-3 py-2">
      <div className="mb-1.5 flex items-baseline gap-2 px-1">
        <span className="min-w-0 truncate text-[12px] font-semibold text-fg-2">{t("disk.listTitle", { where: title, n: files.length })}</span>
        <span className="tabular shrink-0 text-[12px] text-fg-3">{formatSize(total)}</span>
        <span className="ml-auto hidden shrink-0 text-[12px] text-fg-4 @[640px]:inline">{t("disk.listNote")}</span>
      </div>
      {files.map((file, i) => {
        const type = fileType(file.name);
        const Icon = TYPE_ICON[type];
        return (
          <button
            key={file.path.join("/")}
            type="button"
            title={`${file.folder}/${file.name}`}
            onClick={() => void api.diskReveal(file.path, false).catch(() => {})}
            onContextMenu={() => onTarget(asNode(file))}
            className="group flex w-full items-center gap-3 rounded-md px-1 py-1.5 text-left hover:bg-hover"
          >
            <span className="tabular w-6 shrink-0 text-right text-[12px] text-fg-4">{i + 1}</span>
            <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-hover text-fg-2 group-hover:bg-active">
              <Icon size={16} strokeWidth={1.75} aria-hidden />
            </span>
            <span className="min-w-0 flex-1">
              <span className="flex items-baseline gap-2">
                <span className="min-w-0 truncate text-[13px] font-medium">{file.name}</span>
                <span className="shrink-0 text-[12px] text-fg-4">{t(`disk.type.${type}` as Key)}</span>
              </span>
              <span className="block truncate text-[12px] text-fg-3">{file.folder}</span>
            </span>
            <span className="hidden w-[84px] shrink-0 text-right text-[12px] text-fg-3 @[560px]:block">
              {file.modifiedMs ? formatDate(new Date(file.modifiedMs).toISOString(), language) : ""}
            </span>
            <span className="flex w-[110px] shrink-0 flex-col items-end gap-1">
              <span className="tabular text-[13px] font-medium">{formatSize(file.size)}</span>
              <span className="block h-[3px] w-full overflow-hidden rounded-full bg-hover">
                <span className="block h-full rounded-full bg-accent-fill" style={{ width: `${(file.size / biggest) * 100}%` }} />
              </span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** 鼠标停着时的提示：名字、大小、占比、文件数；AI 可用时还有它是做什么的。 */
function Tooltip({ node, x, y, total, ai }: { node: DiskNode; x: number; y: number; total: number; ai: boolean }) {
  const t = useT();
  const label = useLabel();
  const key = node.path.join("/");
  const text = useDisk((state) => state.explanations[key]);
  const asking = useDisk((state) => Boolean(state.asking[key]));
  const share = total > 0 ? (node.size / total) * 100 : 0;
  const explain = ai && explainable(node);
  const width = explain ? 300 : 240;
  const height = explain ? 150 : 90;
  // 靠右下时往左上翻，不出窗口
  const left = x + width > window.innerWidth ? x - width - 8 : x + 14;
  const top = y + height > window.innerHeight ? y - height + 10 : y + 14;
  return (
    <div className="pointer-events-none fixed z-50 rounded-md bg-panel px-2.5 py-1.5 shadow-pop" style={{ left, top, maxWidth: width }}>
      <div className="truncate text-[13px] font-medium">{label(node)}</div>
      <div className="tabular text-[12px] text-fg-3">
        {formatSize(node.size)} · {share >= 0.1 ? `${share.toFixed(1)}%` : "<0.1%"}
      </div>
      <div className="tabular text-[12px] text-fg-3">{t("disk.files", { n: node.files.toLocaleString() })}</div>
      {node.kind === "dir" && !node.hasChildren && <div className="text-[12px] text-fg-3">{t("disk.noDeeper")}</div>}
      {explain && (text || asking) && (
        <div className="mt-1.5 border-t border-line pt-1.5 text-[12px] leading-relaxed text-fg-2">
          {text ? (
            text
          ) : (
            <span className="flex items-center gap-1.5 text-fg-3">
              <LoaderCircle size={12} strokeWidth={2} className="animate-spin" aria-hidden />
              {t("disk.aiLooking")}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

/** 色块的右键菜单：打开、在文件夹中显示、复制路径、移到废纸篓。小文件、其他只能看所在的文件夹。 */
function useNodeMenu(node: DiskNode | null): MenuEntry[] {
  const t = useT();
  const language = useLanguage();
  const notify = useApp((state) => state.notify);
  const { open, path: current, refresh } = useDisk();
  if (!node) return [];
  const label = node.kind === "small" ? t("disk.smallFiles") : node.kind === "other" ? t("disk.other", { n: node.name }) : node.name;
  const real = node.path.length > 0 || node === useDisk.getState().view;
  const where = node.path.length > 0 ? node.path : current;
  const attempt = async (run: () => Promise<unknown>) => {
    try {
      await run();
    } catch (reason) {
      notify(errorText(language, reason), "error");
    }
  };

  if (!real) {
    return [{ label: t("disk.revealParent"), icon: FolderOpen, onSelect: () => void attempt(() => api.diskReveal(current, false)) }];
  }
  const trash = async () => {
    const { ok } = await confirm({
      title: t("disk.trashTitle", { name: label, bin: t(isMac ? "disk.bin.mac" : "disk.bin.other") }),
      body: t("disk.trashBody", { size: formatSize(node.size), files: node.files.toLocaleString(), bin: t(isMac ? "disk.bin.mac" : "disk.bin.other") }),
      confirmLabel: t("disk.trash"),
      danger: true,
    });
    if (!ok) return;
    try {
      const freed = await api.diskTrash(node.path);
      notify(t("disk.trashed", { name: label, size: formatSize(freed) }));
      await refresh();
    } catch (reason) {
      const text = String(reason);
      if (text.startsWith("disk_protected:")) notify(t(`disk.protected.${text.slice("disk_protected:".length)}` as Key), "error");
      else notify(errorText(language, reason), "error");
    }
  };
  return [
    node.kind === "dir"
      ? { label: t("disk.openHere"), icon: ChartPie, disabled: !node.hasChildren || node === useDisk.getState().view, onSelect: () => void open(node.path) }
      : { label: t("disk.openFile"), icon: ExternalLink, onSelect: () => void attempt(() => api.diskReveal(where, true)) },
    { label: t(isMac ? "disk.revealMac" : "disk.reveal"), icon: FolderOpen, onSelect: () => void attempt(() => api.diskReveal(where, false)) },
    {
      label: t("disk.copyPath"),
      icon: Copy,
      onSelect: () =>
        void attempt(async () => {
          await navigator.clipboard.writeText(await api.diskFullPath(where));
          notify(t("disk.copied"));
        }),
    },
    { label: t(isMac ? "disk.trashMac" : "disk.trashOther"), icon: Trash2, danger: true, separated: true, disabled: node.path.length === 0, onSelect: () => void trash() },
  ];
}
