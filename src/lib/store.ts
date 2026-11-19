// 应用状态：仓库状态、索引数据、已安装软件、当前页面、筛选条件、导入进度、提示。

import { create } from "zustand";
import { api, onCheckProgress, onDownloadProgress, onImportProgress } from "./api";
import { type Key, type Translate, type Vars, errorText, setLangPacks, translate } from "./i18n";
import { hasUpdate } from "./library";
import { applyTheme } from "./theme";
import type { Asset, CheckProgress, InstalledApp, LangPack, Language, Library, Platform, Progress, Settings, Status, Storage } from "./types";

export type Route = "library" | "installed" | "storage";
const ROUTES: Route[] = ["library", "installed", "storage"];

export type Scope =
  | { kind: "all" }
  | { kind: "starred" }
  | { kind: "updates" }
  | { kind: "uncategorized" }
  | { kind: "category"; id: string };
export type InstallFilter = "all" | "installed" | "missing";

/** 更新进行到哪一步了。 */
export interface AppUpdate {
  /**
   * idle：没事；checking：正在检查；upToDate：检查过了，已是最新；available：有新版本，等用户决定；
   * downloading：正在下载；ready：下载好了，等用户点安装（Windows）；installed：装好了，重启后生效；error：出错了。
   */
  phase: "idle" | "checking" | "upToDate" | "available" | "downloading" | "ready" | "installed" | "error";
  version: string;
  /** 下载进度 0–1；不知道总大小时为 null。 */
  progress: number | null;
  error: string;
  /** 用户点了“稍后”，这次运行不再弹提示。 */
  dismissed: boolean;
}

export const NO_UPDATE: AppUpdate = { phase: "idle", version: "", progress: null, error: "", dismissed: false };

/** 一个正在下载或排队的文件。 */
export interface DownloadState {
  softwareId: string;
  name: string;
  received: number;
  total: number | null;
}

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error";
}

interface AppState {
  status: Status | null;
  library: Library | null;
  route: Route;
  scope: Scope;
  search: string;
  platform: Platform | "all";
  installFilter: InstallFilter;
  selectedId: string | null;
  progress: Progress | null;
  dropping: boolean;
  toast: Toast | null;
  settingsOpen: boolean;

  /** 这台电脑上已安装的软件。还没列出来时是 null。 */
  installed: InstalledApp[] | null;
  installedLoading: boolean;
  /** 已安装软件占用的空间，统计完才有。 */
  sizes: Record<string, number>;
  /** 仓库里的软件 id → 对应的已安装软件 id。 */
  links: Record<string, string>;
  selectedInstalledId: string | null;

  /** 正在下载或排队的文件，按下载地址。 */
  downloads: Record<string, DownloadState>;
  /** 正在检查更新时的进度。 */
  checking: CheckProgress | null;
  /** 打开着的智能分类预览：要分类的软件。 */
  classifyIds: string[] | null;
  /** 打开着的“关联在线源”对话框：给哪个软件关联；null 表示从在线源新添一个软件。 */
  sourceDialog: { softwareId: string | null } | null;
  /** 语言包变了就加一，让界面重新取文字。 */
  langRevision: number;
  /** ShibaPack 自己的更新进行到哪一步了。 */
  appUpdate: AppUpdate;

  init: () => Promise<void>;
  setRoute: (route: Route) => void;
  setScope: (scope: Scope) => void;
  select: (id: string | null) => void;
  patch: (
    patch: Partial<
      Pick<
        AppState,
        | "search"
        | "platform"
        | "installFilter"
        | "dropping"
        | "settingsOpen"
        | "selectedInstalledId"
        | "classifyIds"
        | "sourceDialog"
      >
    >,
  ) => void;
  notify: (text: string, kind?: Toast["kind"]) => void;
  /** 执行一次修改，成功后用返回的索引替换当前数据；失败时弹出提示。 */
  apply: (task: () => Promise<Library>) => Promise<boolean>;
  /** 执行一个不返回索引的操作，失败时弹出提示。 */
  attempt: <T>(task: () => Promise<T>) => Promise<T | undefined>;
  chooseRepo: (path: string) => Promise<void>;
  importPaths: (paths: string[], mode?: Storage) => Promise<void>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  /** 重新列出已安装的软件，并更新和仓库的对应关系。 */
  loadInstalled: () => Promise<void>;
  /** 检查所有关联了在线源的软件有没有新版本。 */
  checkUpdates: () => Promise<void>;
  /** 下载在线源上的一个安装包并收进仓库。 */
  downloadAsset: (softwareId: string, asset: Asset) => Promise<void>;
  /** 把仓库根目录里散落的安装包归位。 */
  tidyRoot: () => Promise<void>;
  /** 读入或更新语言包。 */
  useLangPacks: (packs: LangPack[]) => void;
}

function routeFromHash(): Route {
  const name = location.hash.replace(/^#\/?/, "") as Route;
  return ROUTES.includes(name) ? name : "library";
}

function fallbackLanguage(): Language {
  try {
    const saved = localStorage.getItem("shibapack.lang");
    if (saved === "zh-CN" || saved === "en-US") return saved;
  } catch {
    // 读不到就按浏览器语言
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

let toastSeq = 0;
// 开发模式下 React 会把启动逻辑跑两遍，这里保证只初始化一次
let started = false;
// 后台任务的状态：补图标、取简介
let fillingIcons = false;
const askedSummaries = new Set<string>();
const summaryQueue: string[] = [];
let summaryWorkers = 0;
let summariesOffline = false;
const SUMMARY_WORKERS = 3;

export const useApp = create<AppState>()((set, get) => {
  const language = () => get().library?.settings.language ?? fallbackLanguage();

  const refreshLinks = async () => {
    if (get().installed === null || get().library === null) return;
    try {
      set({ links: await api.matchInstalled() });
    } catch {
      // 对应关系只是锦上添花，取不到就保持原样
    }
  };

  /** 给没有图标的软件补图标。后端一次处理几个，这里一直调到没有剩余为止。 */
  const fillIcons = async () => {
    // 等已安装软件列出来再开始：图标可能要从那边借
    if (fillingIcons || get().installed === null) return;
    fillingIcons = true;
    try {
      for (let round = 0; round < 500; round++) {
        const result = await api.fillIcons();
        const current = get().library;
        if (result.library && current) set({ library: { ...current, software: result.library.software } });
        if (!result.more) break;
      }
    } catch {
      // 补不上就继续用首字图标
    } finally {
      fillingIcons = false;
    }
  };

  /** 联网取简介：同时最多 3 个，每个名字只问一次。 */
  const pumpSummaries = () => {
    while (summaryWorkers < SUMMARY_WORKERS && summaryQueue.length > 0 && !summariesOffline) {
      const id = summaryQueue.shift()!;
      summaryWorkers++;
      api
        .fetchSummary(id)
        .then((library) => {
          const current = get().library;
          // 只取回简介这一部分，免得用旧的快照盖掉这期间的其他改动
          if (current) set({ library: { ...current, summaries: library.summaries } });
        })
        .catch((error) => {
          // 网络不通：这次运行不再尝试，下次启动再说
          if (String(error).startsWith("offline")) {
            summariesOffline = true;
            summaryQueue.length = 0;
          }
        })
        .finally(() => {
          summaryWorkers--;
          pumpSummaries();
        });
    }
  };

  const queueSummaries = (library: Library) => {
    if (!library.settings.fetchSummaries || summariesOffline) return;
    for (const software of library.software) {
      if (software.name in library.summaries || askedSummaries.has(software.name)) continue;
      askedSummaries.add(software.name);
      summaryQueue.push(software.id);
    }
    pumpSummaries();
  };

  const adopt = (library: Library) => {
    applyTheme(library.settings.theme, library.settings.accent);
    try {
      localStorage.setItem("shibapack.lang", library.settings.language);
    } catch {
      // 存不了也不影响使用
    }
    // 选中的软件、筛选用的分类如果已经不在了，就退回默认
    const state = get();
    const selectedId = library.software.some((s) => s.id === state.selectedId) ? state.selectedId : null;
    const scopeGone =
      state.scope.kind === "category" && !library.categories.some((c) => c.id === (state.scope as { id: string }).id);
    set({ library, selectedId, ...(scopeGone ? { scope: { kind: "all" } as Scope } : {}) });
    void refreshLinks();
    void fillIcons();
    queueSummaries(library);
  };

  return {
    status: null,
    library: null,
    route: routeFromHash(),
    scope: { kind: "all" },
    search: "",
    platform: "all",
    installFilter: "all",
    selectedId: null,
    progress: null,
    dropping: false,
    toast: null,
    settingsOpen: false,
    installed: null,
    installedLoading: false,
    sizes: {},
    links: {},
    selectedInstalledId: null,
    downloads: {},
    checking: null,
    classifyIds: null,
    sourceDialog: null,
    langRevision: 0,
    appUpdate: NO_UPDATE,

    init: async () => {
      if (started) return;
      started = true;
      window.addEventListener("hashchange", () => set({ route: routeFromHash() }));
      void onImportProgress((progress) => set({ progress: progress.phase === "done" ? null : progress }));
      void onCheckProgress((progress) => set({ checking: progress.total > 0 ? progress : null }));
      void onDownloadProgress((progress) => {
        const downloads = { ...get().downloads };
        if (progress.done) delete downloads[progress.url];
        else downloads[progress.url] = { softwareId: progress.softwareId, name: progress.name, received: progress.received, total: progress.total };
        set({ downloads });
      });
      try {
        const status = await api.getStatus();
        set({ status });
        if (status.configured) {
          adopt(await api.getLibrary());
          if (status.recovered !== null) {
            const key: Key = status.recovered ? "recovered.backup" : "recovered.empty";
            get().notify(translate(language(), key, { name: status.recovered }), "error");
          }
          // 已安装软件和语言包在后台取，不挡着界面出来
          void get().loadInstalled();
          void api.listLangPacks().then(get().useLangPacks, () => {});
        }
      } catch (error) {
        get().notify(errorText(language(), error), "error");
      }
    },

    setRoute: (route) => {
      location.hash = `/${route}`;
      // 搜索框是两个列表共用的，换页面时清空，免得带着上一页的搜索词
      if (route !== get().route) set({ search: "" });
      set({ route });
    },

    setScope: (scope) => {
      get().setRoute("library");
      set({ scope });
    },

    select: (id) => set({ selectedId: id }),
    patch: (patch) => set(patch),

    notify: (text, kind = "info") => {
      const id = ++toastSeq;
      set({ toast: { id, text, kind } });
      // 出错的提示多留一会儿
      setTimeout(() => get().toast?.id === id && set({ toast: null }), kind === "error" ? 8000 : 3200);
    },

    apply: async (task) => {
      try {
        adopt(await task());
        return true;
      } catch (error) {
        get().notify(errorText(language(), error), "error");
        return false;
      }
    },

    attempt: async (task) => {
      try {
        return await task();
      } catch (error) {
        get().notify(errorText(language(), error), "error");
        return undefined;
      }
    },

    chooseRepo: async (path) => {
      const status = await get().attempt(() => api.setRepoRoot(path));
      if (!status) return;
      askedSummaries.clear();
      set({ status, selectedId: null, scope: { kind: "all" }, links: {} });
      await get().apply(() => api.getLibrary());
      if (get().installed === null) void get().loadInstalled();
      void api.listLangPacks().then(get().useLangPacks, () => {});
    },

    importPaths: async (paths, mode) => {
      const library = get().library;
      if (!library || paths.length === 0 || get().progress) return;
      set({ progress: { phase: "collect", current: 0, total: 0, file: "" } });
      try {
        const result = await api.importPaths(paths, mode ?? library.settings.defaultStorage);
        adopt(result.library);
        const { imported, duplicates, failed, items } = result.report;
        if (items.length === 0) {
          get().notify(translate(language(), "import.none"));
        } else {
          get().notify(translate(language(), "import.done", { imported, duplicates, failed }), failed ? "error" : "info");
          // 只导入了一个软件时直接选中它
          const ids = new Set(items.filter((i) => i.status === "imported").map((i) => i.softwareId));
          if (ids.size === 1) set({ selectedId: [...ids][0], route: "library" });
        }
      } catch (error) {
        get().notify(errorText(language(), error), "error");
      } finally {
        set({ progress: null });
      }
    },

    saveSettings: async (patch) => {
      const library = get().library;
      if (!library) return;
      const settings = { ...library.settings, ...patch };
      // 先改界面再存盘，切主题不用等磁盘
      applyTheme(settings.theme, settings.accent);
      set({ library: { ...library, settings } });
      await get().apply(() => api.setSettings(settings));
    },

    loadInstalled: async () => {
      if (get().installedLoading) return;
      set({ installedLoading: true });
      try {
        const installed = await api.listInstalled();
        const selectedInstalledId = installed.some((app) => app.id === get().selectedInstalledId)
          ? get().selectedInstalledId
          : null;
        set({ installed, selectedInstalledId });
        void refreshLinks();
        void fillIcons();
        // 占用空间统计得慢，列表先出来，大小随后补上
        void api.measureInstalled().then(
          (sizes) => set({ sizes }),
          () => {},
        );
      } catch (error) {
        get().notify(errorText(language(), error), "error");
        if (get().installed === null) set({ installed: [] });
      } finally {
        set({ installedLoading: false });
      }
    },

    checkUpdates: async () => {
      const library = get().library;
      if (!library || get().checking) return;
      if (!library.software.some((s) => s.subscription)) {
        get().notify(translate(language(), "updates.none"));
        return;
      }
      set({ checking: { current: 0, total: 1, name: "" } });
      try {
        const result = await api.checkUpdates();
        adopt(result.library);
        const updates = result.library.software.filter(hasUpdate).length;
        const key: Key = result.rateLimited ? "updates.rateLimited" : result.failed > 0 ? "updates.failed" : "updates.done";
        get().notify(translate(language(), key, { checked: result.checked, updates, failed: result.failed }), result.rateLimited ? "error" : "info");
      } catch (error) {
        get().notify(errorText(language(), error), "error");
      } finally {
        set({ checking: null });
      }
    },

    downloadAsset: async (softwareId, asset) => {
      if (asset.url in get().downloads) return;
      // 先占个位：前面有别的文件在下的话，它会排队
      set({ downloads: { ...get().downloads, [asset.url]: { softwareId, name: asset.name, received: 0, total: asset.size } } });
      try {
        adopt(await api.downloadAsset(softwareId, asset.url));
        get().notify(translate(language(), "download.done", { name: asset.name }));
      } catch (error) {
        // 自己点的取消不用再提示
        if (!String(error).startsWith("cancelled")) get().notify(errorText(language(), error), "error");
      } finally {
        const downloads = { ...get().downloads };
        delete downloads[asset.url];
        set({ downloads });
      }
    },

    tidyRoot: async () => {
      try {
        const result = await api.tidyRoot();
        adopt(result.library);
        const { imported, duplicates, items } = result.report;
        get().notify(translate(language(), items.length === 0 && duplicates === 0 ? "tidy.none" : "tidy.done", { imported, duplicates }));
      } catch (error) {
        get().notify(errorText(language(), error), "error");
      }
    },

    useLangPacks: (packs) => {
      setLangPacks(packs);
      set({ langRevision: get().langRevision + 1 });
    },
  };
});

// 开发时改了这个文件就整页重载：这里的状态是模块级的，只做热替换会留下一个没初始化的空状态
if (import.meta.hot) import.meta.hot.accept(() => location.reload());

/** 当前语言。仓库还没打开时用上次的语言或系统语言。 */
export function useLanguage(): Language {
  return useApp((state) => state.library?.settings.language) ?? fallbackLanguage();
}

/** 取翻译函数：`const t = useT(); t("common.save")`。 */
export function useT(): Translate {
  const language = useLanguage();
  // 导入语言包后这个数会变，界面跟着重新取一遍文字
  useApp((state) => state.langRevision);
  return (key: Key, vars?: Vars) => translate(language, key, vars);
}
