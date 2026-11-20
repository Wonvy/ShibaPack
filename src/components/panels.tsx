// 首次启动的引导页、设置对话框。

import { ArchiveRestore, FolderInput, FolderOpen, Globe, HardDrive, Info, type LucideIcon, ScrollText, SlidersHorizontal, Sparkles, TriangleAlert } from "lucide-react";
import { type KeyboardEvent, type ReactNode, useState } from "react";
import { api, pickJsonFile, pickSavePath } from "../lib/api";
import { type Key, effectivePack, errorText, languages } from "../lib/i18n";
import { useApp, useLanguage, useT } from "../lib/store";
import { ACCENTS, ACCENT_NAMES } from "../lib/theme";
import { checkForUpdate, startUpdate } from "../lib/updater";
import type { Theme } from "../lib/types";

/** 设置里“导入文件时”的三种做法：复制进仓库、移动进仓库、只记录位置。 */
type ImportMode = "copy" | "move" | "indexed";
import { AiSettingsForm } from "./AiSettings";
import { FailureLogPage } from "./FailureLog";
import { BackupPage, RepoLocation, StoragePage } from "./StorageSettings";
import { Logo } from "./chrome";
import { Button, Modal, Segmented, Select, SettingRow as Row, Switch, cx } from "./ui";

/** 还没选仓库文件夹时显示。选好之前别的界面都不出现。 */
export function SetupGuide() {
  const t = useT();
  const language = useLanguage();
  const chooseRepo = useApp((state) => state.chooseRepo);
  const lastRoot = useApp((state) => state.status?.lastRoot);
  const openError = useApp((state) => state.status?.openError);
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center p-8">
      <div className="flex w-full max-w-[420px] flex-col items-center text-center">
        <Logo size={56} />
        {lastRoot && openError && (
          // 上次的仓库没打开（NAS 没连上、索引是新版本写的……）：说清楚，而不是像第一次用一样
          <div className="mt-5 w-full rounded-lg border border-line bg-panel p-3 text-left">
            <div className="flex items-center gap-2 text-[13px] font-medium text-warning">
              <TriangleAlert size={15} strokeWidth={1.75} aria-hidden className="shrink-0" />
              {t("setup.failedTitle")}
            </div>
            <p className="mt-1.5 mb-0 text-[12px] break-all text-fg-2">{lastRoot}</p>
            <p className="mt-1.5 mb-0 text-[12px] leading-relaxed text-fg-2">
              {openError === "missing" ? t("setup.missing") : errorText(language, openError)}
            </p>
            <Button className="mt-2.5" onClick={() => void chooseRepo(lastRoot)}>
              {t("setup.retry")}
            </Button>
          </div>
        )}
        <h1 className="mt-5 mb-0 text-[17px] font-semibold">{t("setup.title")}</h1>
        <p className="mt-2 mb-0 text-[13px] leading-relaxed text-fg-2">{t("setup.body")}</p>
        <Button variant="primary" className="mt-6 h-8 px-3.5" onClick={() => void chooseRepo()}>
          <FolderOpen size={15} strokeWidth={1.75} aria-hidden />
          {t("setup.choose")}
        </Button>
        <p className="mt-3 mb-0 text-[12px] text-fg-3">{t("setup.note")}</p>
      </div>
    </div>
  );
}

/** 设置的各页。左边一列选项卡（按组隔开一点），右边是这一页的内容。 */
type SettingsPage = "general" | "library" | "storage" | "backup" | "online" | "ai" | "logs" | "about";
const PAGES: { page: SettingsPage; icon: LucideIcon; label: Key; hint?: Key; group: number }[] = [
  { page: "general", icon: SlidersHorizontal, label: "settings.general", group: 0 },
  { page: "library", icon: FolderInput, label: "settings.library", group: 1 },
  { page: "storage", icon: HardDrive, label: "settings.storage", group: 1 },
  { page: "backup", icon: ArchiveRestore, label: "settings.backup", group: 1 },
  { page: "online", icon: Globe, label: "settings.online", group: 2 },
  { page: "ai", icon: Sparkles, label: "settings.ai", hint: "settings.aiHint", group: 2 },
  { page: "logs", icon: ScrollText, label: "settings.logs", group: 3 },
  { page: "about", icon: Info, label: "settings.about", group: 3 },
];

export function SettingsDialog() {
  const t = useT();
  const open = useApp((state) => state.settingsOpen);
  const settings = useApp((state) => state.library?.settings);
  const patch = useApp((state) => state.patch);
  // 这次运行里记着上次看的是哪一页，再打开时还在那一页
  const [page, setPage] = useState<SettingsPage>("general");
  if (!settings) return null;
  const current = PAGES.find((p) => p.page === page) ?? PAGES[0];

  // 上下方向键在各页之间切换
  const onNavKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const at = PAGES.findIndex((p) => p.page === page);
    const next = PAGES[(at + step + PAGES.length) % PAGES.length].page;
    setPage(next);
    document.getElementById(`settings-tab-${next}`)?.focus();
  };

  const body: Record<SettingsPage, () => ReactNode> = {
    general: () => <GeneralPage />,
    library: () => <LibraryPage />,
    storage: () => <StoragePage />,
    backup: () => <BackupPage />,
    online: () => <OnlinePage />,
    ai: () => <AiSettingsForm />,
    logs: () => <FailureLogPage />,
    about: () => <AboutPage />,
  };

  return (
    <Modal open={open} onClose={() => patch({ settingsOpen: false })} title={t("settings.title")} width={720} bare>
      <div className="flex h-[min(560px,calc(80vh-44px))] min-h-0 border-t border-line">
        <div
          role="tablist"
          aria-label={t("settings.title")}
          aria-orientation="vertical"
          onKeyDown={onNavKey}
          className="flex w-[168px] shrink-0 flex-col gap-0.5 overflow-y-auto rounded-bl-xl border-r border-line bg-side p-2"
        >
          {PAGES.map((p, i) => {
            const selected = p.page === page;
            return (
              <button
                key={p.page}
                type="button"
                role="tab"
                id={`settings-tab-${p.page}`}
                aria-selected={selected}
                aria-controls="settings-panel"
                tabIndex={selected ? 0 : -1}
                onClick={() => setPage(p.page)}
                className={cx(
                  "flex h-8 shrink-0 items-center gap-2 rounded-md px-2 text-left text-[13px] transition-colors",
                  i > 0 && PAGES[i - 1].group !== p.group && "mt-3",
                  selected ? "bg-active text-fg" : "text-fg-2 hover:bg-hover hover:text-fg",
                )}
              >
                <p.icon size={15} strokeWidth={1.75} aria-hidden className={cx("shrink-0", selected ? "text-fg" : "text-fg-3")} />
                <span className="truncate">{t(p.label)}</span>
              </button>
            );
          })}
        </div>
        <div
          role="tabpanel"
          id="settings-panel"
          aria-labelledby={`settings-tab-${page}`}
          className="min-h-0 min-w-0 flex-1 overflow-y-auto px-5 pt-4 pb-5"
        >
          <h2 className="m-0 text-[15px] font-semibold">{t(current.label)}</h2>
          {current.hint && <p className="m-0 mt-1 text-[12px] leading-relaxed text-fg-3">{t(current.hint)}</p>}
          <div className={current.page === "ai" ? "mt-4" : "mt-1"}>{body[page]()}</div>
        </div>
      </div>
    </Modal>
  );
}

/** 通用：主题、强调色、界面语言和语言包。 */
function GeneralPage() {
  const t = useT();
  const settings = useApp((state) => state.library!.settings);
  const save = useApp((state) => state.saveSettings);
  const attempt = useApp((state) => state.attempt);
  const notify = useApp((state) => state.notify);
  const useLangPacks = useApp((state) => state.useLangPacks);
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
    <>
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
              aria-label={t(`accent.${name}` as Key)}
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
    </>
  );
}

/** 仓库：仓库在哪、导入文件时怎么放、离线目录。 */
function LibraryPage() {
  const t = useT();
  const settings = useApp((state) => state.library!.settings);
  const save = useApp((state) => state.saveSettings);
  return (
    <>
      <RepoLocation />
      <Row label={t("settings.defaultStorage")} hint={t("settings.storageHint")}>
        <Segmented<ImportMode>
          value={settings.defaultStorage === "indexed" ? "indexed" : settings.moveOnImport ? "move" : "copy"}
          onChange={(mode) => void save({ defaultStorage: mode === "indexed" ? "indexed" : "managed", moveOnImport: mode === "move" })}
          options={[
            { value: "copy", label: t("settings.managed") },
            { value: "move", label: t("settings.moved") },
            { value: "indexed", label: t("settings.indexed") },
          ]}
        />
      </Row>
      <Row label={t("settings.autoClassifyOnImport")} hint={t("settings.autoClassifyOnImportHint")}>
        <Switch checked={settings.autoClassifyOnImport} onChange={(autoClassifyOnImport) => void save({ autoClassifyOnImport })} label={t("settings.autoClassifyOnImport")} />
      </Row>
      <Row label={t("settings.catalog")} hint={t("settings.catalogHint")}>
        <Switch checked={settings.offlineCatalog} onChange={(offlineCatalog) => void save({ offlineCatalog })} label={t("settings.catalog")} />
      </Row>
    </>
  );
}

/** 联网：取简介和图标、自动翻译。 */
function OnlinePage() {
  const t = useT();
  const settings = useApp((state) => state.library!.settings);
  const save = useApp((state) => state.saveSettings);
  return (
    <>
      <Row label={t("settings.summaries")} hint={t("settings.summariesHint")}>
        <Switch checked={settings.fetchSummaries} onChange={(fetchSummaries) => void save({ fetchSummaries })} label={t("settings.summaries")} />
      </Row>
      <Row label={t("settings.theSvgIcons")} hint={t("settings.theSvgIconsHint")}>
        <Switch checked={settings.theSvgIcons} disabled={!settings.fetchSummaries} onChange={(theSvgIcons) => void save({ theSvgIcons })} label={t("settings.theSvgIcons")} />
      </Row>
      <Row label={t("settings.translate")} hint={t("settings.translateHint")}>
        <Switch checked={settings.translateSummaries} onChange={(translateSummaries) => void save({ translateSummaries })} label={t("settings.translate")} />
      </Row>
    </>
  );
}

/** 关于：版本号、自动更新、检查更新。 */
function AboutPage() {
  const t = useT();
  const settings = useApp((state) => state.library!.settings);
  const save = useApp((state) => state.saveSettings);
  const version = useApp((state) => state.status?.version);
  const update = useApp((state) => state.appUpdate);
  const updateNote: Partial<Record<typeof update.phase, string>> = {
    checking: t("update.checking"),
    upToDate: t("update.upToDate"),
    available: t("update.available", { version: update.version }),
    downloading: t("update.downloading", { version: update.version }),
    ready: t("update.ready", { version: update.version }),
    installed: t("update.installed", { version: update.version }),
    error: t("update.failed", { message: update.error }),
  };
  return (
    <>
      <div className="flex items-center gap-3 border-b border-line py-4">
        <Logo size={40} />
        <div className="min-w-0 flex-1">
          <div className="text-[14px] font-semibold">ShibaPack</div>
          <div className="text-[12px] text-fg-3">{t("settings.version", { version: version ?? "" })}</div>
        </div>
        {update.phase === "available" ? (
          <Button onClick={() => void startUpdate()}>{t("update.install")}</Button>
        ) : (
          <Button busy={update.phase === "checking"} disabled={["downloading", "ready", "installed"].includes(update.phase)} onClick={() => void checkForUpdate(true)}>
            {t("update.check")}
          </Button>
        )}
      </div>
      {updateNote[update.phase] && (
        <p className={cx("m-0 border-b border-line py-3 text-[12px] leading-relaxed", update.phase === "error" ? "text-danger" : "text-fg-2")}>
          {updateNote[update.phase]}
        </p>
      )}
      <Row label={t("update.auto")} hint={t("update.autoHint")}>
        <Switch checked={settings.autoUpdate} onChange={(autoUpdate) => void save({ autoUpdate })} label={t("update.auto")} />
      </Row>
    </>
  );
}
