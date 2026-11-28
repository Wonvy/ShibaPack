// 主题和强调色。颜色写到 <html> 的 CSS 变量上，其余样式都从变量取色。

import type { Settings, Theme } from "./types";

type ResolvedTheme = Exclude<Theme, "system">;
type ThemeSettings = Pick<Settings, "theme" | "paperTheme">;
export const DEFAULT_ACCENT = "sky";
const WHITE_INK = "#ffffff";
const DARK_INK = "#0a0b0d";

/** 色板和控件底色沿用 ShibaPlan 的原色，文字使用对应背景下的高对比色。 */
export const ACCENTS = {
  linear: { hex: "#5e6ad2", lightInk: "#40499b", darkInk: "#a5adff", onAccent: WHITE_INK },
  indigo: { hex: "#4f46e5", lightInk: "#4338ca", darkInk: "#a5a0ff", onAccent: WHITE_INK },
  violet: { hex: "#7c3aed", lightInk: "#6d28d9", darkInk: "#c4a5ff", onAccent: WHITE_INK },
  sky: { hex: "#0284c7", lightInk: "#075985", darkInk: "#7dd3fc", onAccent: DARK_INK },
  emerald: { hex: "#059669", lightInk: "#076046", darkInk: "#6ee7b7", onAccent: DARK_INK },
  amber: { hex: "#f59e0b", lightInk: "#78430b", darkInk: "#fbbf24", onAccent: DARK_INK },
  orange: { hex: "#ea580c", lightInk: "#8f3411", darkInk: "#fdba74", onAccent: DARK_INK },
  rose: { hex: "#e11d48", lightInk: "#a81236", darkInk: "#fda4af", onAccent: WHITE_INK },
  slate: { hex: "#52525b", lightInk: "#42424a", darkInk: "#bcbcc5", onAccent: WHITE_INK },
} as const;

type AccentName = keyof typeof ACCENTS;
type HexColor = `#${string}`;
type Accent = AccentName | HexColor;
export const ACCENT_NAMES = Object.keys(ACCENTS) as AccentName[];
const LEGACY_ACCENTS = new Map<string, AccentName>([
  ["blue", "linear"],
  ["purple", "violet"],
  ["green", "emerald"],
  ["pink", "rose"],
]);

export function normalizeHex(input: string): HexColor | null {
  const raw = input.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{6}$/.test(raw)) return `#${raw}`;
  if (/^[0-9a-f]{3}$/.test(raw)) return `#${[...raw].map((digit) => digit.repeat(2)).join("")}`;
  return null;
}

export function resolveAccent(name: string): Accent {
  if (Object.hasOwn(ACCENTS, name)) return name as AccentName;
  const legacy = LEGACY_ACCENTS.get(name);
  if (legacy) return legacy;
  const hex = normalizeHex(name);
  const preset = hex && ACCENT_NAMES.find((id) => ACCENTS[id].hex === hex);
  return preset || hex || DEFAULT_ACCENT;
}

function isPreset(accent: Accent): accent is AccentName {
  return Object.hasOwn(ACCENTS, accent);
}

export function accentHex(accent: Accent): HexColor {
  return isPreset(accent) ? ACCENTS[accent].hex : accent;
}

function whiteContrast(hex: HexColor): number {
  const channels = [1, 3, 5].map((offset) => {
    const srgb = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
  });
  const luminance = channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  return 1.05 / (luminance + 0.05);
}

export function swatchInk(hex: HexColor): string {
  return whiteContrast(hex) >= 3 ? WHITE_INK : DARK_INK;
}

const systemLight = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: light)") : null;
let current: { theme: Theme; accent: Accent } = { theme: "paper", accent: DEFAULT_ACCENT };

export function themeChoice({ theme, paperTheme }: ThemeSettings): Theme {
  return theme === "light" && paperTheme ? "paper" : theme;
}

function resolveTheme(theme: Theme): ResolvedTheme {
  if (theme === "light" || theme === "dark" || theme === "paper") return theme;
  return systemLight?.matches ? "light" : "dark";
}

function paint() {
  const resolved = resolveTheme(current.theme);
  const preset = isPreset(current.accent) ? ACCENTS[current.accent] : null;
  const hex = accentHex(current.accent);
  const ink = resolved === "dark" ? preset?.darkInk : preset?.lightInk;
  const root = document.documentElement;
  root.dataset.theme = resolved;
  root.style.setProperty("--accent", hex);
  root.style.setProperty("--accent-ink", ink ?? `color-mix(in srgb, ${hex} 40%, ${resolved === "dark" ? WHITE_INK : DARK_INK})`);
  root.style.setProperty("--on-accent", preset?.onAccent ?? (whiteContrast(hex) >= 4.5 ? WHITE_INK : "#000000"));
}

export function applyTheme(settings: ThemeSettings & Pick<Settings, "accent">) {
  const theme = themeChoice(settings);
  current = { theme, accent: resolveAccent(settings.accent) };
  paint();
  // 记在本机，下次启动在样式加载前就能定好主题，不闪一下
  try {
    localStorage.setItem("shibapack.theme", theme);
  } catch {
    // 存不了也不影响使用
  }
}

systemLight?.addEventListener("change", paint);
