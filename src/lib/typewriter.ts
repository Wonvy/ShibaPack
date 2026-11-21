import { useEffect, useState } from "react";

/** 渐进显示新生成的文字；尊重系统的减少动态效果设置。 */
export function useTypewriter(text: string, animate: boolean): string {
  const [progress, setProgress] = useState({ text: "", count: 0 });
  useEffect(() => {
    if (!animate || !text || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const characters = Array.from(text);
    const started = performance.now();
    const tick = () => setProgress({ text, count: Math.min(characters.length, Math.floor((performance.now() - started) / 22) + 1) });
    tick();
    const timer = window.setInterval(tick, 40);
    const finish = window.setTimeout(() => window.clearInterval(timer), characters.length * 22 + 40);
    return () => { window.clearInterval(timer); window.clearTimeout(finish); };
  }, [text, animate]);
  if (!animate || !text || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return text;
  return progress.text === text ? Array.from(text).slice(0, progress.count).join("") : "";
}
