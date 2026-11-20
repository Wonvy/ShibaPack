// 网格视图里鼠标停在一个软件上一会儿，旁边浮出它的简介（系统自带的 title 提示出得慢、字小、不能换行）。
// 按下鼠标、滚动、右键、离开时收起；没有简介时什么都不显示。

import { type MouseEvent, type ReactNode, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

/** 停多久才浮出来。 */
const DELAY = 450;
const WIDTH = 280;
const GAP = 6;

/**
 * 给一个元素加悬停简介：把 `handlers` 展开到元素上，把 `card` 放在旁边渲染。
 * `content` 为 null 时不显示。按下鼠标时也会收起：调用方自己的 onPointerDown 要接着调 `handlers.onPointerDown`。
 */
export function useHoverCard(content: ReactNode | null) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const hide = () => {
    window.clearTimeout(timer.current);
    setAnchor(null);
  };

  useEffect(() => () => window.clearTimeout(timer.current), []);
  useEffect(() => {
    if (!anchor) return;
    window.addEventListener("scroll", hide, true);
    window.addEventListener("blur", hide);
    window.addEventListener("keydown", hide);
    return () => {
      window.removeEventListener("scroll", hide, true);
      window.removeEventListener("blur", hide);
      window.removeEventListener("keydown", hide);
    };
  }, [anchor]);

  const handlers = {
    onMouseEnter: (event: MouseEvent<HTMLElement>) => {
      if (content === null) return;
      const element = event.currentTarget;
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => element.isConnected && setAnchor(element.getBoundingClientRect()), DELAY);
    },
    onMouseLeave: hide,
    onPointerDown: hide,
    onContextMenu: hide,
  };
  const card = anchor && content !== null ? createPortal(<Card anchor={anchor}>{content}</Card>, document.body) : null;
  return { handlers, card };
}

function Card({ anchor, children }: { anchor: DOMRect; children: ReactNode }) {
  const left = Math.min(Math.max(anchor.left + anchor.width / 2 - WIDTH / 2, 8), window.innerWidth - WIDTH - 8);
  // 下面放得下就放下面，否则放上面
  const below = anchor.bottom + GAP + 140 < window.innerHeight;
  const style = below ? { left, top: anchor.bottom + GAP, width: WIDTH } : { left, bottom: window.innerHeight - anchor.top + GAP, width: WIDTH };
  return (
    <div role="tooltip" style={style} className="pointer-events-none fixed z-50 animate-fade rounded-lg bg-panel px-3 py-2.5 text-left shadow-pop">
      {children}
    </div>
  );
}

/** 悬停简介的内容：名字、发布者、简介。 */
export function SummaryCard({ name, publisher, text }: { name: string; publisher?: string; text: string }) {
  return (
    <>
      <div className="truncate text-[13px] font-semibold text-fg">{name}</div>
      {publisher && <div className="truncate text-[12px] text-fg-3">{publisher}</div>}
      <p className="m-0 mt-1.5 line-clamp-6 text-[12px] leading-relaxed whitespace-pre-line text-fg-2">{text}</p>
    </>
  );
}
