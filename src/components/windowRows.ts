import { type RefObject, useLayoutEffect, useState } from "react";

export const TABLE_ROW_HEIGHT = 52;
const OVERSCAN_ROWS = 8;
const WINDOW_THRESHOLD = 250;

interface Viewport {
  top: number;
  height: number;
}

export function useWindowRows(ref: RefObject<HTMLDivElement | null>, rows: number) {
  const enabled = rows >= WINDOW_THRESHOLD;
  const [viewport, setViewport] = useState<Viewport>({ top: 0, height: window.innerHeight });
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element || !enabled) return;
    let frame: number | undefined;
    const measure = () => {
      frame = undefined;
      const top = element.scrollTop;
      const height = element.clientHeight;
      setViewport((current) => current.top === top && current.height === height ? current : { top, height });
    };
    const update = () => { frame ??= requestAnimationFrame(measure); };
    measure();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    element.addEventListener("scroll", update, { passive: true });
    return () => {
      observer.disconnect();
      element.removeEventListener("scroll", update);
      if (frame !== undefined) cancelAnimationFrame(frame);
    };
  }, [enabled, ref]);
  return enabled ? viewport : null;
}

export function visibleRows(viewport: Viewport | null, count: number, offset = 0) {
  if (!viewport) return { first: 0, last: count };
  const clamp = (index: number) => Math.max(0, Math.min(count, index));
  return {
    first: clamp(Math.floor((viewport.top - offset) / TABLE_ROW_HEIGHT) - OVERSCAN_ROWS),
    last: clamp(Math.ceil((viewport.top + viewport.height - offset) / TABLE_ROW_HEIGHT) + OVERSCAN_ROWS),
  };
}

export function scrollToRow(element: HTMLDivElement, top: number) {
  if (top < element.scrollTop) element.scrollTop = top;
  else if (top + TABLE_ROW_HEIGHT > element.scrollTop + element.clientHeight) {
    element.scrollTop = top + TABLE_ROW_HEIGHT - element.clientHeight;
  }
}
