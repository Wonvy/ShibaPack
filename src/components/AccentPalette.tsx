import { Check, RotateCcw } from "lucide-react";
import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useApp, useT } from "../lib/store";
import { ACCENTS, ACCENT_NAMES, accentHex, applyTheme, DEFAULT_ACCENT, normalizeHex, resolveAccent, swatchInk } from "../lib/theme";
import { Button, TextInput, cx } from "./ui";

function restoreSavedAccent() {
  const settings = useApp.getState().library?.settings;
  if (settings) applyTheme(settings);
}

export function AccentPalette() {
  const t = useT();
  const saved = resolveAccent(useApp((state) => state.library!.settings.accent));
  const save = useApp((state) => state.saveSettings);
  const [draft, setDraft] = useState<string | null>(null);
  const previewing = useRef(false);
  const picker = useRef<HTMLInputElement>(null);
  const hex = draft === null ? null : normalizeHex(draft);
  const selected = hex ? resolveAccent(hex) : saved;
  const swatch = accentHex(selected);
  const invalid = draft !== null && hex === null;

  function preview(value: string) {
    setDraft(value);
    const settings = useApp.getState().library?.settings;
    if (!settings) return;
    const color = normalizeHex(value);
    if (color || previewing.current) applyTheme(color ? { ...settings, accent: color } : settings);
    previewing.current = color !== null;
  }

  function discard() {
    setDraft(null);
    if (previewing.current) restoreSavedAccent();
    previewing.current = false;
  }

  function commit(value: string) {
    const next = resolveAccent(value);
    if (next === saved) {
      discard();
      return;
    }
    setDraft(null);
    previewing.current = false;
    void save({ accent: next });
  }

  function finishEditing() {
    if (hex) commit(hex);
    else discard();
  }

  const commitPicker = useEffectEvent((value: string) => commit(value));
  useEffect(() => {
    const input = picker.current;
    if (!input) return;
    // 原生拾色器拖动时预览，确认时才写入设置。
    const onChange = () => commitPicker(input.value);
    input.addEventListener("change", onChange);
    return () => {
      input.removeEventListener("change", onChange);
      if (previewing.current) restoreSavedAccent();
    };
  }, []);

  return (
    <section className="@container border-b border-line pt-5 pb-4" aria-labelledby="accent-title">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <h3 id="accent-title" className="text-xs font-medium">{t("settings.accent")}</h3>
          <p className="text-xs text-fg-3">{t("settings.accentHint")}</p>
        </div>
        <Button variant="ghost" size="sm" data-accent-choice onClick={() => commit(DEFAULT_ACCENT)}>
          <RotateCcw size={12} aria-hidden />
          {t("settings.accentReset")}
        </Button>
      </div>
      <div role="group" aria-labelledby="accent-title" className="mt-3 grid grid-cols-3 gap-2 @[400px]:grid-cols-5">
        {ACCENT_NAMES.map((name) => {
          const label = t(`accent.${name}`);
          const color = ACCENTS[name].hex;
          const active = selected === name;
          return (
            <button
              key={name}
              type="button"
              aria-pressed={active}
              title={`${label} · ${color.toUpperCase()}`}
              data-accent-choice
              onClick={() => commit(name)}
              className={cx(
                "flex h-8 min-w-0 items-center gap-2 rounded-lg px-2.5 text-left text-xs transition-colors",
                active
                  ? "bg-(--accent-choice-selected) font-medium text-fg shadow-xs"
                  : "bg-(--accent-choice) text-fg-3 hover:bg-(--accent-choice-selected) hover:text-fg",
              )}
            >
              <span aria-hidden className="grid size-4 shrink-0 place-items-center rounded-full" style={{ backgroundColor: color, color: swatchInk(color) }}>
                {active && <Check size={10} strokeWidth={2.5} />}
              </span>
              <span className="truncate">{label}</span>
            </button>
          );
        })}
      </div>
      <div className="mt-3 flex items-center gap-2.5 rounded-xl bg-(--accent-choice) p-2.5">
        <label
          className="relative h-8 w-10 shrink-0 overflow-hidden rounded-lg shadow-xs focus-within:ring-2 focus-within:ring-accent"
          style={{ backgroundColor: swatch }}
          title={t("settings.accentPicker")}
        >
          <input
            ref={picker}
            type="color"
            aria-label={t("settings.accentPicker")}
            data-accent-choice
            className="absolute inset-0 size-full cursor-pointer opacity-0"
            value={swatch}
            onInput={(event) => preview(event.currentTarget.value)}
          />
        </label>
        <TextInput
          className="h-8 min-w-0 flex-1 bg-panel font-mono text-xs"
          aria-label={t("settings.accentHex")}
          aria-invalid={invalid || undefined}
          aria-describedby={invalid ? "accent-hex-error" : undefined}
          value={draft ?? accentHex(saved).toUpperCase()}
          onChange={(event) => preview(event.target.value)}
          onBlur={(event) => {
            if (!event.relatedTarget?.closest("[data-accent-choice]")) finishEditing();
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !invalid) finishEditing();
          }}
        />
        <span className="shrink-0 pr-1 text-xs text-fg-3">HEX</span>
      </div>
      {invalid && <p id="accent-hex-error" className="mt-1.5 text-xs text-danger">{t("settings.accentInvalid")}</p>}
    </section>
  );
}
