import { useLayoutEffect, useRef } from "react";

type Position = { x: number; y: number };

/** CSS Grid 先完成自动换列，再用位移把卡片从旧位置滑到新位置。 */
export function useGridMotion(enabled: boolean, items: readonly unknown[], folded?: ReadonlySet<string>) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const viewport = ref.current;
    if (!enabled || !viewport || typeof Element.prototype.animate !== "function") return;

    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const running = new Map<HTMLElement, Animation>();
    let previous = new Map<HTMLElement, Position>();

    const reflow = () => {
      const bounds = viewport.getBoundingClientRect();
      const viewTop = viewport.scrollTop;
      const viewBottom = viewTop + viewport.clientHeight;
      const originX = bounds.left - viewport.scrollLeft;
      const originY = bounds.top - viewTop;
      const next = new Map<HTMLElement, Position>();
      const changes: { tile: HTMLElement; delta: Position | null }[] = [];

      // 先读完几何信息，再统一更新动画，避免重复触发布局。
      for (const tile of viewport.querySelectorAll<HTMLElement>("[data-grid-tile]")) {
        const box = tile.getBoundingClientRect();
        const transform = running.has(tile) ? getComputedStyle(tile).transform : "none";
        const matrix = transform === "none" ? null : new DOMMatrixReadOnly(transform);
        const offsetX = matrix?.m41 ?? 0;
        const offsetY = matrix?.m42 ?? 0;
        const position = { x: box.left + box.width / 2 - originX - offsetX, y: box.top - originY - offsetY };
        next.set(tile, position);
        const old = previous.get(tile);
        if (!old || (Math.abs(old.x - position.x) < 0.5 && Math.abs(old.y - position.y) < 0.5)) continue;

        const visible = Math.min(old.y, position.y) < viewBottom && Math.max(old.y, position.y) + box.height > viewTop;
        changes.push({ tile, delta: visible && !reducedMotion.matches
          ? { x: old.x + offsetX - position.x, y: old.y + offsetY - position.y }
          : null });
      }

      for (const { tile, delta } of changes) {
        running.get(tile)?.cancel();
        running.delete(tile);
        if (!delta) continue;
        const animation = tile.animate(
          [{ transform: `translate3d(${delta.x}px, ${delta.y}px, 0)` }, { transform: "none" }],
          { duration: 220, easing: "cubic-bezier(0.2, 0.75, 0.25, 1)", fill: "backwards" },
        );
        running.set(tile, animation);
        animation.onfinish = () => {
          if (running.get(tile) === animation) running.delete(tile);
        };
      }
      previous = next;
    };

    reflow();
    const observer = new ResizeObserver(reflow);
    observer.observe(viewport);
    return () => {
      observer.disconnect();
      for (const animation of running.values()) animation.cancel();
    };
  }, [enabled, items, folded]);

  return ref;
}
