import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";
import type { ThemeMode } from "../types";

export interface Toast {
  id: number;
  text: string;
  tone: "default" | "success" | "error";
}

let toastSeq = 0;

function storedWidth(): number {
  const value = Number(localStorage.getItem("shibapack.sidebarWidth"));
  return value >= 140 && value <= 360 ? value : 180;
}

function storedDetailWidth(): number {
  const value = Number(localStorage.getItem("shibapack.detailWidth"));
  return value >= 280 && value <= 560 ? value : 340;
}

export type PackageView = "list" | "grid" | "folder";

function parseView(key: string): PackageView {
  const value = localStorage.getItem(key);
  return value === "grid" || value === "folder" ? value : "list";
}

function storedView(): PackageView {
  return parseView("shibapack.packageView");
}

function storedInstalledView(): PackageView {
  return parseView("shibapack.installedView");
}

export const useUiStore = defineStore("ui", () => {
  const theme = ref<ThemeMode>("system");
  const accent = ref("#a25bff");
  const toasts = ref<Toast[]>([]);
  const sidebarWidth = ref(storedWidth());
  const detailWidth = ref(storedDetailWidth());
  const packageView = ref<PackageView>(storedView());
  const installedView = ref<PackageView>(storedInstalledView());
  const settingsOpen = ref(false);

  const resolvedTheme = computed<"dark" | "light">(() => {
    if (theme.value !== "system") return theme.value;
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  });

  function applyAppearance() {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme.value;
    root.style.setProperty("--accent", accent.value);
    root.style.setProperty("--accent-strong", accent.value);
    root.style.setProperty("--accent-soft", `color-mix(in srgb, ${accent.value} 16%, transparent)`);
  }

  watch([theme, accent, resolvedTheme], applyAppearance, { immediate: true });
  watch(sidebarWidth, (value) => localStorage.setItem("shibapack.sidebarWidth", String(value)));
  watch(detailWidth, (value) => localStorage.setItem("shibapack.detailWidth", String(value)));
  watch(packageView, (value) => localStorage.setItem("shibapack.packageView", value));
  watch(installedView, (value) => localStorage.setItem("shibapack.installedView", value));

  window.matchMedia("(prefers-color-scheme: light)").addEventListener("change", applyAppearance);

  function toast(text: string, tone: Toast["tone"] = "default") {
    const id = ++toastSeq;
    toasts.value.push({ id, text, tone });
    window.setTimeout(() => {
      toasts.value = toasts.value.filter((item) => item.id !== id);
    }, 3200);
  }

  return {
    theme,
    accent,
    toasts,
    sidebarWidth,
    detailWidth,
    packageView,
    installedView,
    settingsOpen,
    resolvedTheme,
    applyAppearance,
    toast,
  };
});
