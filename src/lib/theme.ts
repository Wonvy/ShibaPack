// 主题和强调色。颜色写到 <html> 的 CSS 变量上，其余样式都从变量取色。

import type { Settings, Theme } from "./types";

type Shade = { accent: string; strong: string };
type ResolvedTheme = Exclude<Theme, "system">;
type ThemeSettings = Pick<Settings, "theme" | "paperTheme">;
const DEFAULT_ACCENT = "sky";

/** 强调色预设：每种主题下各一组，保证文字和按钮都看得清。 */
export const ACCENTS: Record<string, Record<ResolvedTheme, Shade>> = {
  sky: {
    dark: { accent: "#38bdf8", strong: "#0369a1" },
    light: { accent: "#0369a1", strong: "#075985" },
    paper: { accent: "#0369a1", strong: "#075985" },
  },
  blue: {
    dark: { accent: "#5aa2ff", strong: "#2b74d9" },
    light: { accent: "#1f6fe0", strong: "#1a5fc4" },
    paper: { accent: "#43578b", strong: "#394c7d" },
  },
  purple: {
    dark: { accent: "#a78bfa", strong: "#7557e8" },
    light: { accent: "#6d43e0", strong: "#5b34c9" },
    paper: { accent: "#69508b", strong: "#584174" },
  },
  green: {
    dark: { accent: "#4cc38a", strong: "#23875a" },
    light: { accent: "#1f8a57", strong: "#18754a" },
    paper: { accent: "#39694d", strong: "#305b42" },
  },
  orange: {
    dark: { accent: "#f0a04b", strong: "#b96a14" },
    light: { accent: "#c26a0a", strong: "#a85a06" },
    paper: { accent: "#955022", strong: "#82441d" },
  },
  pink: {
    dark: { accent: "#f472b6", strong: "#c93d82" },
    light: { accent: "#d6337f", strong: "#b8286b" },
    paper: { accent: "#9a4761", strong: "#853b53" },
  },
};

export const ACCENT_NAMES = Object.keys(ACCENTS);

const systemLight = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: light)") : null;
let current: { theme: Theme; accent: string } = { theme: "paper", accent: DEFAULT_ACCENT };

export function themeChoice({ theme, paperTheme }: ThemeSettings): Theme {
  return theme === "light" && paperTheme ? "paper" : theme;
}

export function resolveTheme(theme: Theme): ResolvedTheme {
  if (theme === "light" || theme === "dark" || theme === "paper") return theme;
  return systemLight?.matches ? "light" : "dark";
}

function paint() {
  const resolved = resolveTheme(current.theme);
  const shade = (ACCENTS[current.accent] ?? ACCENTS[DEFAULT_ACCENT])[resolved];
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.style.setProperty("--accent", shade.accent);
  root.style.setProperty("--accent-strong", shade.strong);
}

export function applyTheme(settings: ThemeSettings & Pick<Settings, "accent">) {
  const theme = themeChoice(settings);
  current = { theme, accent: settings.accent };
  paint();
  // 记在本机，下次启动在样式加载前就能定好主题，不闪一下
  try {
    localStorage.setItem("shibapack.theme", theme);
  } catch {
    // 存不了也不影响使用
  }
}

systemLight?.addEventListener("change", paint);
