// 首次启动的引导页、设置对话框。

import { FolderOpen } from "lucide-react";
import { api, pickFolder, pickJsonFile, pickSavePath } from "../lib/api";
import { effectivePack, languages } from "../lib/i18n";
import { useApp, useT } from "../lib/store";
import { ACCENTS, ACCENT_NAMES } from "../lib/theme";
import { checkForUpdate, startUpdate } from "../lib/updater";
import type { Storage, Theme } from "../lib/types";
import { Logo } from "./chrome";
import { Button, Modal, Segmented, Select, Switch, cx } from "./ui";

/** 还没选仓库文件夹时显示。选好之前别的界面都不出现。 */
export function SetupGuide() {
  const t = useT();
  const chooseRepo = useApp((state) => state.chooseRepo);
  const choose = async () => {
    const folder = await pickFolder();
    if (folder) await chooseRepo(folder);
  };
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center p-8">
      <div className="flex w-full max-w-[420px] flex-col items-center text-center">
        <Logo size={56} />
        <h1 className="mt-5 mb-0 text-[17px] font-semibold">{t("setup.title")}</h1>
        <p className="mt-2 mb-0 text-[13px] leading-relaxed text-fg-2">{t("setup.body")}</p>
        <Button variant="primary" className="mt-6 h-8 px-3.5" onClick={() => void choose()}>
          <FolderOpen size={15} strokeWidth={1.75} aria-hidden />
          {t("setup.choose")}
        </Button>
        <p className="mt-3 mb-0 text-[12px] text-fg-3">{t("setup.note")}</p>
      </div>
    </div>
  );
}

export function SettingsDialog() {
  const t = useT();
  const open = useApp((state) => state.settingsOpen);
  const settings = useApp((state) => state.library?.settings);
  const version = useApp((state) => state.status?.version);
  const patch = useApp((state) => state.patch);
  const save = useApp((state) => state.saveSettings);
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const useLangPacks = useApp((state) => state.useLangPacks);
  const update = useApp((state) => state.appUpdate);
  if (!settings) return null;
  const updateNote: Partial<Record<typeof update.phase, string>> = {
    checking: t("update.checking"),
    upToDate: t("update.upToDate"),
    available: t("update.available", { version: update.version }),
    downloading: t("update.downloading", { version: update.version }),
    ready: t("update.ready", { version: update.version }),
    installed: t("update.installed", { version: update.version }),
    error: t("update.failed", { message: update.error }),
  };
  const dark = document.documentElement.dataset.theme !== "light";
  const available = languages();
  const current = available.find((l) => l.code === settings.language);

  const exportPack = async () => {
    const dest = await pickSavePath(`shibapack-${settings.language}.json`);
    if (dest && (await attempt(() => api.exportLangPack(dest, effectivePack(settings.language)).then(() => true)))) {
      notify(t("settings.langExported"));
    }
  };
  const importPack = async () => {
    const src = await pickJsonFile();
    const packs = src && (await attempt(() => api.importLangPack(src)));
    if (packs) {
      useLangPacks(packs);
      notify(t("settings.langImported"));
    }
  };
  const removePack = async () => {
    const packs = await attempt(() => api.removeLangPack(settings.language));
    if (!packs) return;
    useLangPacks(packs);
    // 移除的是一门导入的语言时，退回中文
    if (!["zh-CN", "en-US"].includes(settings.language)) await save({ language: "zh-CN" });
  };

  return (
    <Modal open={open} onClose={() => patch({ settingsOpen: false })} title={t("settings.title")} width={460}>
      <Row label={t("settings.theme")}>
        <Segmented<Theme>
          value={settings.theme}
          onChange={(theme) => void save({ theme })}
          options={[
            { value: "system", label: t("theme.system") },
            { value: "dark", label: t("theme.dark") },
            { value: "light", label: t("theme.light") },
          ]}
        />
      </Row>
      <Row label={t("settings.accent")}>
        <div className="flex gap-2">
          {ACCENT_NAMES.map((name) => (
            <button
              key={name}
              type="button"
              aria-label={name}
              aria-pressed={settings.accent === name}
              onClick={() => void save({ accent: name })}
              style={{ background: ACCENTS[name][dark ? "dark" : "light"].accent }}
              className={cx("size-5 rounded-full", settings.accent === name && "ring-2 ring-fg ring-offset-2 ring-offset-panel")}
            />
          ))}
        </div>
      </Row>
      <Row label={t("settings.language")} hint={t("settings.langHint")}>
        <Select compact aria-label={t("settings.language")} value={settings.language} onChange={(event) => void save({ language: event.target.value })}>
          {available.map((language) => (
            <option key={language.code} value={language.code}>
              {language.name}
            </option>
          ))}
        </Select>
        <div className="mt-2 flex w-full justify-end gap-2">
          {current?.custom && (
            <Button variant="ghost" className="text-danger" onClick={() => void removePack()}>
              {t("settings.langRemove")}
            </Button>
          )}
          <Button onClick={() => void exportPack()}>{t("settings.langExport")}</Button>
          <Button onClick={() => void importPack()}>{t("settings.langImport")}</Button>
        </div>
      </Row>
      <Row label={t("settings.defaultStorage")} hint={t("settings.storageHint")}>
        <Segmented<Storage>
          value={settings.defaultStorage}
          onChange={(defaultStorage) => void save({ defaultStorage })}
          options={[
            { value: "managed", label: t("settings.managed") },
            { value: "indexed", label: t("settings.indexed") },
          ]}
        />
      </Row>
      <Row label={t("settings.summaries")} hint={t("settings.summariesHint")}>
        <Switch
          checked={settings.fetchSummaries}
          onChange={(fetchSummaries) => void save({ fetchSummaries })}
          label={t("settings.summaries")}
        />
      </Row>
      <Row label={t("settings.catalog")} hint={t("settings.catalogHint")}>
        <Switch checked={settings.offlineCatalog} onChange={(offlineCatalog) => void save({ offlineCatalog })} label={t("settings.catalog")} />
      </Row>
      <Row label={t("update.auto")} hint={t("update.autoHint")}>
        <Switch checked={settings.autoUpdate} onChange={(autoUpdate) => void save({ autoUpdate })} label={t("update.auto")} />
      </Row>
      <div className="flex min-h-12 items-center gap-3 pt-3 text-[12px] text-fg-3">
        <span className="shrink-0">ShibaPack · {t("settings.version", { version: version ?? "" })}</span>
        <span className={cx("min-w-0 flex-1 truncate text-right", update.phase === "error" && "text-danger")} title={updateNote[update.phase]}>
          {updateNote[update.phase]}
        </span>
        {update.phase === "available" ? (
          <Button onClick={() => void startUpdate()}>{t("update.install")}</Button>
        ) : (
          <Button busy={update.phase === "checking"} disabled={["downloading", "ready", "installed"].includes(update.phase)} onClick={() => void checkForUpdate(true)}>
            {t("update.check")}
          </Button>
        )}
      </div>
    </Modal>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-line py-3">
      <div className="flex flex-wrap items-center justify-between gap-x-4">
        <span className="text-[13px]">{label}</span>
        {children}
      </div>
      {hint && <p className="m-0 mt-2 text-[12px] leading-relaxed text-fg-3">{hint}</p>}
    </div>
  );
}
