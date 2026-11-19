// 拖拽：把软件拖到分类上，或者拖动分类调整顺序和层级。
//
// 用指针事件自己实现，不用浏览器自带的拖放：桌面窗口为了接收从系统拖进来的文件，
// 在 Windows 上会让网页自带的拖放失效。

import type { PointerEvent as ReactPointerEvent } from "react";
import { create } from "zustand";

export type DragPayload = { kind: "software" | "category"; id: string; label: string };
export type DropZone = "inside" | "before" | "after";

/** 落点。`category` 为 null 表示“未分类”。 */
export interface DropTarget {
  category: string | null;
  zone: DropZone;
}

interface DragState {
  payload: DragPayload | null;
  target: DropTarget | null;
  x: number;
  y: number;
}

export const useDrag = create<DragState>(() => ({ payload: null, target: null, x: 0, y: 0 }));

/** 按住移动超过这么多像素才算拖拽，否则仍然是一次普通的点击。 */
const THRESHOLD = 5;

/**
 * 在可拖动元素的 onPointerDown 里调用。
 * 可以落下的元素用 `data-drop="category:<id>"` 或 `data-drop="uncategorized"` 标出来。
 * `accepts` 用来排除不合法的落点（比如把分类拖进它自己的下级）。
 */
export function beginDrag(
  event: ReactPointerEvent,
  payload: DragPayload,
  onDrop: (target: DropTarget) => void,
  accepts: (target: DropTarget) => boolean = () => true,
) {
  if (event.button !== 0) return;
  const start = { x: event.clientX, y: event.clientY };
  let dragging = false;

  const locate = (x: number, y: number): DropTarget | null => {
    const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    const raw = element?.dataset.drop;
    if (!element || !raw) return null;
    let target: DropTarget;
    if (raw === "uncategorized") {
      if (payload.kind !== "software") return null;
      target = { category: null, zone: "inside" };
    } else if (payload.kind === "software") {
      target = { category: raw.slice("category:".length), zone: "inside" };
    } else {
      // 拖的是分类：落在一行的上沿是排到它前面，下沿是排到后面，中间是放进去做下级
      const rect = element.getBoundingClientRect();
      const ratio = (y - rect.top) / rect.height;
      target = { category: raw.slice("category:".length), zone: ratio < 0.28 ? "before" : ratio > 0.72 ? "after" : "inside" };
    }
    return accepts(target) ? target : null;
  };

  const swallowClick = (click: MouseEvent) => {
    click.stopPropagation();
    click.preventDefault();
  };

  const finish = (x: number | null, y: number | null) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    window.removeEventListener("keydown", onKey);
    document.body.style.cursor = "";
    const target = dragging && x !== null && y !== null ? locate(x, y) : null;
    useDrag.setState({ payload: null, target: null });
    if (dragging) {
      // 松手时浏览器还会补发一次 click，吞掉它，免得拖完顺手把东西选中了
      window.addEventListener("click", swallowClick, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallowClick, { capture: true }), 0);
    }
    if (target) onDrop(target);
  };

  const onMove = (move: PointerEvent) => {
    if (!dragging) {
      if (Math.hypot(move.clientX - start.x, move.clientY - start.y) < THRESHOLD) return;
      dragging = true;
      document.body.style.cursor = "grabbing";
    }
    useDrag.setState({ payload, target: locate(move.clientX, move.clientY), x: move.clientX, y: move.clientY });
  };
  const onUp = (up: PointerEvent) => finish(up.clientX, up.clientY);
  const onCancel = () => finish(null, null);
  const onKey = (key: KeyboardEvent) => key.key === "Escape" && finish(null, null);

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancel);
  window.addEventListener("keydown", onKey);
}
