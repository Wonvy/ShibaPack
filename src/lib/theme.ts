// 主题和强调色。颜色写到 <html> 的 CSS 变量上，其余样式都从变量取色。

import type { Theme } from "./types";

type Shade = { accent: string; strong: string };

/** 强调色预设：每种主题下各一组，保证文字和按钮都看得清。 */
export const ACCENTS: Record<string, { dark: Shade; light: Shade }> = {
  blue: { dark: { accent: "#5aa2ff", strong: "#2b74d9" }, light: { accent: "#1f6fe0", strong: "#1a5fc4" } },
  purple: { dark: { accent: "#a78bfa", strong: "#7557e8" }, light: { accent: "#6d43e0", strong: "#5b34c9" } },
  green: { dark: { accent: "#4cc38a", strong: "#23875a" }, light: { accent: "#1f8a57", strong: "#18754a" } },
  orange: { dark: { accent: "#f0a04b", strong: "#b96a14" }, light: { accent: "#c26a0a", strong: "#a85a06" } },
  pink: { dark: { accent: "#f472b6", strong: "#c93d82" }, light: { accent: "#d6337f", strong: "#b8286b" } },
};

export const ACCENT_NAMES = Object.keys(ACCENTS);

const systemLight = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: light)") : null;
let current: { theme: Theme; accent: string } = { theme: "system", accent: "blue" };

function paint() {
  const resolved = current.theme === "system" ? (systemLight?.matches ? "light" : "dark") : current.theme;
  const shade = (ACCENTS[current.accent] ?? ACCENTS.blue)[resolved];
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.style.setProperty("--accent", shade.accent);
  root.style.setProperty("--accent-strong", shade.strong);
}

export function applyTheme(theme: Theme, accent: string) {
  current = { theme, accent };
  paint();
  // 记在本机，下次启动在样式加载前就能定好主题，不闪一下
  try {
    localStorage.setItem("shibapack.theme", theme);
  } catch {
    // 存不了也不影响使用
  }
}

systemLight?.addEventListener("change", paint);
