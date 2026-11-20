// 可以调列宽、勾选显示哪几列的表头，软件库和已安装软件的列表共用。
//
// 第一列（名称）一直在，占满剩下的宽度；右边几列的宽度在表头上拖动列与列之间的竖线来改，双击回到默认宽度；
// 右键表头可以勾选显示哪几列。栏太窄、名称放不下时，标了 `autoHide` 的列按数字从小到大暂时收起。
// 列宽和隐藏的列存在 prefs（localStorage）里，只和这台电脑有关。

import { RotateCcw } from "lucide-react";
import { type PointerEvent, type ReactNode, createContext, useEffect, useMemo, useRef, useState } from "react";
import { type TableId, usePrefs } from "../lib/prefs";
import { useT } from "../lib/store";
import { ContextMenu, type MenuEntry, cx } from "./ui";

export interface ColumnDef<K extends string = string> {
  key: K;
  /** 默认宽度和能拖到的范围。min 和 max 一样时不能拖。 */
  initial: number;
  min: number;
  max: number;
  /** 靠右对齐（数字、按钮）。 */
  end?: boolean;
  /** 栏太窄时暂时收起的先后：数字小的先收。不填就不会自动收起。 */
  autoHide?: number;
}

export interface Columns<K extends string = string> {
  /** 现在显示的列，按从左到右的顺序（不含名称）。 */
  visible: K[];
  /** 勾着、但因为栏太窄暂时收起的列。 */
  squeezed: K[];
  widths: Record<K, number>;
  /** 表头和每一行共用的 grid-template-columns。 */
  template: string;
}

/** 名称列至少留这么宽，放不下时才收起别的列。 */
const NAME_MIN = 160;
/** 列与列之间的空隙，和 `gap-x-3` 一致。 */
export const COLUMN_GAP = 12;

/** 行里按这个决定画哪几列。 */
export const ColumnsContext = createContext<Columns>({ visible: [], squeezed: [], widths: {}, template: "minmax(0,1fr)" });

/** 按设置和可用宽度算出现在显示哪几列、各多宽。`available` 是一行里能分给各列的宽度，还没量出来时传 0。 */
export function useTableColumns<K extends string>(table: TableId, defs: ColumnDef<K>[], available: number): Columns<K> {
  const settings = usePrefs((state) => state.tableColumns[table]);
  return useMemo(() => {
    const hidden = settings?.hidden ?? [];
    const custom = settings?.widths ?? {};
    const widths = Object.fromEntries(defs.map((def) => [def.key, custom[def.key] ?? def.initial])) as Record<K, number>;
    const need = (keys: K[]) => NAME_MIN + keys.reduce((sum, key) => sum + widths[key] + COLUMN_GAP, 0);
    let visible = defs.map((def) => def.key).filter((key) => !hidden.includes(key));
    const squeezed: K[] = [];
    const order = defs.filter((def) => def.autoHide !== undefined).sort((a, b) => a.autoHide! - b.autoHide!);
    for (const def of order) {
      if (available <= 0 || need(visible) <= available) break;
      if (!visible.includes(def.key)) continue;
      visible = visible.filter((key) => key !== def.key);
      squeezed.push(def.key);
    }
    const template = ["minmax(0,1fr)", ...visible.map((key) => `${widths[key]}px`)].join(" ");
    return { visible, squeezed, widths, template };
  }, [settings, defs, available]);
}

/**
 * 表头：第一格是名称（`first`），后面是各列。`cells` 里给了的列用它（比如能点着排序的按钮），没给的显示列名。
 * 列与列之间的竖线一直淡淡地画着，拖它改宽度。
 */
export function TableHeader<K extends string>({
  table,
  defs,
  columns,
  labels,
  first,
  firstLabel,
  cells,
  className,
}: {
  table: TableId;
  defs: ColumnDef<K>[];
  columns: Columns<K>;
  labels: Record<K, string>;
  first: ReactNode;
  /** 名称列在右键菜单里的名字。 */
  firstLabel: string;
  cells?: Partial<Record<K, ReactNode>>;
  className?: string;
}) {
  const t = useT();
  const hidden = usePrefs((state) => state.tableColumns[table]?.hidden ?? []);
  const toggleColumn = usePrefs((state) => state.toggleColumn);
  const resetColumns = usePrefs((state) => state.resetColumns);
  const menu: MenuEntry[] = [
    { label: firstLabel, checked: true, disabled: true, onSelect: () => {} },
    ...defs.map((def) => ({
      // 勾着但栏太窄放不下的，说明一下为什么没显示
      label: columns.squeezed.includes(def.key) ? t("lib.col.squeezed", { name: labels[def.key] }) : labels[def.key],
      checked: !hidden.includes(def.key),
      onSelect: () => toggleColumn(table, def.key),
    })),
    { label: t("lib.col.reset"), icon: RotateCcw, separated: true, onSelect: () => resetColumns(table) },
  ];
  const byKey = Object.fromEntries(defs.map((def) => [def.key, def])) as Record<K, ColumnDef<K>>;
  return (
    <ContextMenu items={menu}>
      <div
        className={cx("grid h-8 shrink-0 items-stretch gap-x-3 border-b border-line text-[12px] text-fg-3", className)}
        style={{ gridTemplateColumns: columns.template }}
      >
        <div className="flex min-w-0 items-center">{first}</div>
        {columns.visible.map((key) => {
          const def = byKey[key];
          return (
            <div key={key} className={cx("relative flex min-w-0 items-center", def.end && "justify-end")}>
              {cells?.[key] ?? <span className="truncate">{labels[key]}</span>}
              {def.min !== def.max && <ResizeGrip table={table} def={def} width={columns.widths[key]} />}
            </div>
          );
        })}
      </div>
    </ContextMenu>
  );
}

/** 列左边的拖动条：往左拖这一列变宽、往右拖变窄，名称列跟着让出或收回宽度。双击回到默认宽度。 */
function ResizeGrip({ table, def, width }: { table: TableId; def: ColumnDef; width: number }) {
  const t = useT();
  const setColumnWidth = usePrefs((state) => state.setColumnWidth);
  const [dragging, setDragging] = useState(false);
  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const grip = event.currentTarget;
    const startX = event.clientX;
    grip.setPointerCapture(event.pointerId);
    setDragging(true);
    const move = (next: globalThis.PointerEvent) => setColumnWidth(table, def.key, width - (next.clientX - startX), def);
    const stop = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", stop);
      grip.removeEventListener("pointercancel", stop);
      setDragging(false);
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", stop);
    grip.addEventListener("pointercancel", stop);
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t("lib.col.resize")}
      title={t("lib.col.resize")}
      onPointerDown={onPointerDown}
      onDoubleClick={() => setColumnWidth(table, def.key, null, def)}
      onClick={(event) => event.stopPropagation()}
      // 落在两列之间的空隙里，比竖线本身宽一些，好按住
      className="group/grip absolute top-1 bottom-1 -left-[11px] z-10 flex w-[10px] cursor-col-resize justify-center"
    >
      <span
        className={cx(
          "w-px transition-colors",
          dragging ? "bg-accent" : "bg-line-strong opacity-60 group-hover/grip:bg-accent group-hover/grip:opacity-100",
        )}
      />
    </div>
  );
}

/** 量一个元素的宽度，跟着窗口和分栏变化。 */
export function useWidth<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}
