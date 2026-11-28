// 应用状态：仓库状态、索引数据、已安装软件、当前页面、筛选条件、导入进度、提示。

import { useCallback } from "react";
import { create } from "zustand";
import { aiUsable } from "./ai";
import { api, onCheckProgress, onDownloadProgress, onImportProgress, pickFolder } from "./api";
import { type Key, type Translate, type Vars, errorText, setLangPacks, translate } from "./i18n";
import { filterSoftware, needsTranslation } from "./library";
import { applyTheme } from "./theme";
import { createImportSlice, type ImportSlice } from "./store/imports";
import { createInstalledSlice, type InstalledSlice } from "./store/installed";
import { createSourceSlice, type SourceSlice } from "./store/sources";

export type { ImportDialogState, ImportOptions } from "./store/imports";
export type { DownloadState } from "./store/sources";
import type {
  LangPack,
  Language,
  Library,
  Platform,
  Settings,
  Status,
} from "./types";

export type Route = "home" | "library" | "installed" | "startup" | "cleanup" | "disk" | "paths";
const ROUTES: Route[] = ["home", "library", "installed", "startup", "cleanup", "disk", "paths"];

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
  /** 新版本的更新说明（更新包里带的；没带时“软件更新”窗口去 GitHub 上取）。 */
  notes: string;
  /** 装不上是因为没有权限替换自己（macOS 的“App 管理”限制）：窗口里给出放行和手动安装的办法。 */
  blocked: boolean;
}

export const NO_UPDATE: AppUpdate = { phase: "idle", version: "", progress: null, error: "", dismissed: false, notes: "", blocked: false };

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "error";
}

export interface AppState extends ImportSlice, InstalledSlice, SourceSlice {
  status: Status | null;
  library: Library | null;
  route: Route;
  scope: Scope;
  search: string;
  platform: Platform | "all";
  installFilter: InstallFilter;
  /** 选中的软件；多选时是最先选的那个（Shift 连选从它算起）。 */
  selectedId: string | null;
  /** 多选时所有选中的软件；单选时只有 selectedId 一个。 */
  selectedIds: string[];
  /** 屏幕下方的提示，最新的在最后，最多同时 3 条。新提示不会把还没看完的出错提示挤掉。 */
  toasts: Toast[];
  dismissToast: (id: number) => void;
  settingsOpen: boolean;

  /** 打开着的智能分类预览：要分类的软件。 */
  classifyIds: string[] | null;
  /** 语言包变了就加一，让界面重新取文字。 */
  langRevision: number;
  /** ShibaPack 自己的更新进行到哪一步了。 */
  appUpdate: AppUpdate;
  /** 开着“软件更新”窗口（点底栏的版本号打开）。 */
  updateDialog: boolean;

  init: () => Promise<void>;
  setRoute: (route: Route) => void;
  setScope: (scope: Scope) => void;
  /** 单选一个软件（null 为不选）。 */
  select: (id: string | null) => void;
  /** 多选：`primary` 是连选的起点。 */
  setSelection: (ids: string[], primary: string | null) => void;
  /** 选中一个软件；它被当前的筛选挡住时，先回到“全部软件”并清掉筛选。 */
  reveal: (id: string) => void;
  patch: (
    patch: Partial<
      Pick<
        AppState,
        | "search"
        | "platform"
        | "installFilter"
        | "dropping"
        | "fileDropTarget"
        | "importDialog"
        | "settingsOpen"
        | "selectedInstalledId"
        | "classifyIds"
        | "uninstallTarget"
        | "sourceDialog"
        | "updateDialog"
      >
    >,
  ) => void;
  notify: (text: string, kind?: Toast["kind"]) => void;
  /** 执行一次修改，成功后用返回的索引替换当前数据；失败时弹出提示。 */
  apply: (task: () => Promise<Library>) => Promise<boolean>;
  /** 执行一个不返回索引的操作，失败时弹出提示。 */
  attempt: <T>(task: () => Promise<T>) => Promise<T | undefined>;
  chooseRepo: (path?: string) => Promise<void>;
  /** 窗口回到前台时调用：索引被别处改过就换上新的。 */
  refreshLibrary: () => Promise<void>;
  /**
   * 给一个名字取简介（详情栏里正看着的软件，包括只装在本机、仓库里没有的）。
   * 平常只在没取过、或者需要翻译时才取；`refresh` 为真时不管有没有都重新取。
   */
  requestSummary: (name: string, refresh?: boolean) => void;
  /** 正在取简介的名字。 */
  summaryBusy: Record<string, boolean>;
  /** 这台电脑的钥匙串里存着设置里那个 AI 服务的 API Key。 */
  aiKeySaved: boolean;
  /** 重新看一下钥匙串里有没有 API Key（换了服务、存了密钥之后）。 */
  refreshAiKey: () => Promise<void>;
  /** 正在让 AI 写简介的名字。 */
  aiBusy: Record<string, boolean>;
  /**
   * 让 AI 给这些名字写简介，一个一个来。只有一个时出错直接提示原因，多个时最后汇总。
   * 一次给好几个时，已经有 AI 简介的跳过，不重复花钱；只给一个（点了“AI 写简介”“用 AI 重写”）时一定重新写。
   */
  describeWithAi: (items: { name: string; publisher?: string }[]) => Promise<void>;
  /** 自己写的描述（按名字存）。空字符串表示删掉。 */
  writeSummary: (name: string, text: string) => Promise<boolean>;
  saveSettings: (patch: Partial<Settings>) => Promise<void>;
  /** 读入或更新语言包。 */
  useLangPacks: (packs: LangPack[]) => void;
}

function routeFromHash(): Route {
  const name = location.hash.replace(/^#\/?/, "") as Route;
  // 没有指定页面时打开主页
  return ROUTES.includes(name) ? name : "home";
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
/** 上次查过钥匙串的 AI 服务地址：换了地址才再查。 */
let aiKeyCheckedFor: string | null = null;
// 开发模式下 React 会把启动逻辑跑两遍，这里保证只初始化一次
let started = false;
// 后台任务的状态：补图标、联网找图标、取简介
let fillingIcons = false;
let fillingInfo = false;
/** 本机的图标（安装包里、已安装软件那边）至少补过一轮，之后才联网找，免得网站图标抢在真图标前面。 */
let localIconsDone = false;
/** 软件 id → 这次运行里已经用于联网查找图标的来源和信息。 */
const askedIcons = new Map<string, string>();
const iconQueue = new Set<string>();
let iconWorkers = 0;
/** 连续几次连不上就当作没有网，这次运行不再找。 */
let iconFailures = 0;
const ICON_WORKERS = 3;
const askedSummaries = new Set<string>();
/** 正在导入一批文件。 */
const summaryQueue: { name: string; refresh: boolean }[] = [];
let summaryWorkers = 0;
let summariesOffline = false;
const SUMMARY_WORKERS = 3;

export const useApp = create<AppState>()((set, get) => {
  const language = () => get().library?.settings.language ?? fallbackLanguage();

  const refreshLinks = async () => {
    if (get().installed === null || get().library === null) return;
    try {
      const links = await api.matchInstalled();
      set({ links: Object.fromEntries(Object.entries(links).filter(([, id]) => id in get().installedIndex)) });
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
      localIconsDone = true;
    }
    queueWebIcons();
  };

  /** 用安装包自带的属性（exe、msi 的产品版本、公司名）补版本号和发布者。读过的后端会缓存，不会每次重读。 */
  const fillFileInfo = async () => {
    if (fillingInfo) return;
    fillingInfo = true;
    try {
      for (let round = 0; round < 500; round++) {
        const result = await api.fillFileInfo();
        const current = get().library;
        if (result.library && current) set({ library: { ...current, software: result.library.software } });
        if (!result.more) break;
      }
    } catch {
      // 读不出来就保持原样
    } finally {
      fillingInfo = false;
    }
  };

  /** 给还没有图标的软件用已启用的来源联网找图标，同时最多 3 个。 */
  const queueWebIcons = () => {
    const library = get().library;
    if (!library?.settings.fetchSummaries || !localIconsDone || fillingIcons || iconFailures >= 3) return;
    for (const software of library.software) {
      if (software.iconData) continue;
      const homepage = software.homepage.trim();
      const github = software.subscription?.source === "github" ? software.subscription.reference.trim() : "";
      const theSvgName = library.settings.theSvgIcons ? software.name.trim() : "";
      if (!homepage && !github && !theSvgName) continue;
      const key = JSON.stringify([homepage, github, theSvgName]);
      if (askedIcons.get(software.id) === key) continue;
      askedIcons.set(software.id, key);
      iconQueue.add(software.id);
    }
    pumpIcons();
  };

  const pumpIcons = () => {
    const root = get().status?.repoRoot;
    const settings = get().library?.settings;
    if (!settings?.fetchSummaries) return;
    for (const id of iconQueue) {
      if (iconWorkers >= ICON_WORKERS || iconFailures >= 3) break;
      iconQueue.delete(id);
      iconWorkers++;
      api
        .fetchIcon(id)
        .then((update) => {
          if (get().status?.repoRoot !== root) return;
          iconFailures = 0;
          const current = get().library;
          // 只改这一个软件的图标，免得用旧的快照盖掉这期间的其他改动
          if (update && current) {
            set({ library: { ...current, software: current.software.map((s) => (s.id === id && !s.iconData ? { ...s, ...update } : s)) } });
          }
        })
        .catch((error) => {
          // 一个网站打不开不代表没网；连续几个都失败才停。没找到的下次启动再试
          const current = get();
          if (current.status?.repoRoot === root && current.library?.settings.fetchSummaries
              && current.library.settings.theSvgIcons === settings.theSvgIcons && String(error).startsWith("offline")) {
            iconFailures++;
          }
        })
        .finally(() => {
          iconWorkers--;
          pumpIcons();
        });
    }
  };

  /** 联网取简介：同时最多 3 个，每个名字只问一次。简介按软件名存，仓库里的软件和只装在本机的软件都一样。 */
  const pumpSummaries = () => {
    while (summaryWorkers < SUMMARY_WORKERS && summaryQueue.length > 0 && !summariesOffline) {
      const { name, refresh } = summaryQueue.shift()!;
      summaryWorkers++;
      api
        .fetchSummary(name, refresh)
        .then((library) => {
          const current = get().library;
          // 只取回简介这一部分，免得用旧的快照盖掉这期间的其他改动
          if (current) set({ library: { ...current, summaries: library.summaries } });
        })
        .catch((error) => {
          // 网络不通：这次运行不再自动尝试，下次启动再说（手动点“获取简介”还可以再试）
          if (String(error).startsWith("offline")) {
            summariesOffline = true;
            summaryQueue.length = 0;
            if (refresh) get().notify(errorText(language(), error), "error");
          }
        })
        .finally(() => {
          summaryWorkers--;
          const { [name]: _done, ...rest } = get().summaryBusy;
          set({ summaryBusy: rest });
          pumpSummaries();
        });
    }
  };

  /** 这个名字要不要去取简介（或翻译）：要的话返回去重用的键，不要返回 null。 */
  const summaryWanted = (library: Library, name: string): string | null => {
    const { language: lang, translateSummaries } = library.settings;
    const summary = library.summaries[name];
    if (!summary) return name;
    return translateSummaries && needsTranslation(summary, lang) ? `${name}\u0000${lang}` : null;
  };

  /** 排进取简介的队列。`first` 为真时插到最前面（用户正看着的软件先取）。 */
  const enqueueSummary = (name: string, refresh: boolean, first: boolean) => {
    const item = { name, refresh };
    if (first) summaryQueue.unshift(item);
    else summaryQueue.push(item);
    set({ summaryBusy: { ...get().summaryBusy, [name]: true } });
  };

  // 还没有简介的去取；有简介、但不是界面语言又开着自动翻译的去翻译。每个名字（每种语言）这次运行只问一次
  const queueSummaries = (library: Library) => {
    if (!library.settings.fetchSummaries || summariesOffline) return;
    for (const software of library.software) {
      const key = summaryWanted(library, software.name);
      if (key === null || askedSummaries.has(key)) continue;
      askedSummaries.add(key);
      enqueueSummary(software.name, false, false);
    }
    pumpSummaries();
  };

  const adopt = (library: Library) => {
    applyTheme(library.settings);
    try {
      localStorage.setItem("shibapack.lang", library.settings.language);
    } catch {
      // 存不了也不影响使用
    }
    // 选中的软件、筛选用的分类如果已经不在了，就退回默认
    const state = get();
    const exists = new Set(library.software.map((s) => s.id));
    const selectedIds = state.selectedIds.filter((id) => exists.has(id));
    const selectedId = state.selectedId && exists.has(state.selectedId) ? state.selectedId : (selectedIds[0] ?? null);
    const scopeGone =
      state.scope.kind === "category" && !library.categories.some((c) => c.id === (state.scope as { id: string }).id);
    set({ library, selectedId, selectedIds, ...(scopeGone ? { scope: { kind: "all" } as Scope } : {}) });
    void refreshLinks();
    void fillIcons();
    void fillFileInfo();
    queueWebIcons();
    queueSummaries(library);
    if (library.settings.ai.baseUrl !== aiKeyCheckedFor) void get().refreshAiKey();
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
    selectedIds: [],
    ...createImportSlice(set, get, adopt, language),
    summaryBusy: {},
    aiKeySaved: false,
    aiBusy: {},
    toasts: [],
    settingsOpen: false,
    ...createInstalledSlice(set, get, refreshLinks, fillIcons, language),
    ...createSourceSlice(set, get, adopt, language),
    classifyIds: null,
    langRevision: 0,
    appUpdate: NO_UPDATE,
    updateDialog: false,

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

    select: (id) => set({ selectedId: id, selectedIds: id ? [id] : [] }),
    setSelection: (ids, primary) => set({ selectedIds: ids, selectedId: primary ?? ids[0] ?? null }),
    reveal: (id) => {
      const state = get();
      const library = state.library;
      if (!library) return;
      const visible = filterSoftware(library.software, library.categories, state).some((s) => s.id === id);
      if (!visible) set({ scope: { kind: "all" }, platform: "all", installFilter: "all", search: "" });
      get().setRoute("library");
      get().select(id);
    },
    patch: (patch) => set(patch),

    notify: (text, kind = "info") => {
      const id = ++toastSeq;
      // 同样的话已经在显示：换成新的一条（重新计时），不叠两条
      const others = get().toasts.filter((toast) => toast.text !== text);
      // 超过 3 条时先挤掉最早的普通提示，出错的提示尽量留着
      while (others.length >= 3) {
        const oldest = others.findIndex((toast) => toast.kind !== "error");
        others.splice(oldest >= 0 ? oldest : 0, 1);
      }
      set({ toasts: [...others, { id, text, kind }] });
      // 出错的提示都记进失败日志（在设置的“失败日志”里看）
      if (kind === "error") void api.logFailure(get().route, text).catch(() => {});
      // 出错的提示多留一会儿
      setTimeout(() => get().dismissToast(id), kind === "error" ? 8000 : 3200);
    },

    dismissToast: (id) => set({ toasts: get().toasts.filter((toast) => toast.id !== id) }),

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
      const folder = path ?? await get().attempt(pickFolder);
      if (!folder) return;
      const status = await get().attempt(() => api.setRepoRoot(folder));
      if (!status) return;
      askedSummaries.clear();
      askedIcons.clear();
      iconQueue.clear();
      iconFailures = 0;
      set({ status, selectedId: null, selectedIds: [], scope: { kind: "all" }, links: {} });
      await get().apply(() => api.getLibrary());
      void get().loadInstalled();
      void api.listLangPacks().then(get().useLangPacks, () => {});
    },

    requestSummary: (name, refresh = false) => {
      const library = get().library;
      if (!library || !name.trim() || get().summaryBusy[name]) return;
      if (refresh) {
        // 手动点的：网络刚才不通也再试一次
        summariesOffline = false;
      } else {
        if (!library.settings.fetchSummaries || summariesOffline) return;
        const key = summaryWanted(library, name);
        if (key === null || askedSummaries.has(key)) return;
        askedSummaries.add(key);
      }
      enqueueSummary(name, refresh, true);
      pumpSummaries();
    },

    refreshAiKey: async () => {
      const baseUrl = get().library?.settings.ai.baseUrl ?? "";
      aiKeyCheckedFor = baseUrl;
      const saved = baseUrl.trim() ? await api.aiKeySaved(baseUrl).catch(() => false) : false;
      // 查的这一会儿又换了服务：以后来的那次为准
      if (aiKeyCheckedFor === baseUrl) set({ aiKeySaved: saved });
    },

    writeSummary: async (name, text) => {
      try {
        const library = await api.setSummary(name, text);
        const current = get().library;
        if (current) set({ library: { ...current, summaries: library.summaries } });
        return true;
      } catch (error) {
        get().notify(errorText(language(), error), "error");
        return false;
      }
    },

    describeWithAi: async (items) => {
      const summaries = get().library?.summaries ?? {};
      const batch = items.length > 1;
      const already = batch ? items.filter((item) => summaries[item.name]?.source.startsWith("AI")) : [];
      if (already.length > 0) get().notify(translate(language(), "ai.skipped", { n: already.length }));
      const todo = items.filter((item) => item.name.trim() && !get().aiBusy[item.name] && !already.includes(item));
      if (todo.length === 0) return;
      set({ aiBusy: { ...get().aiBusy, ...Object.fromEntries(todo.map((item) => [item.name, true])) } });
      const lang = language();
      const release = () =>
        set({ aiBusy: Object.fromEntries(Object.entries(get().aiBusy).filter(([name]) => !todo.some((item) => item.name === name))) });
      try {
        if (todo.length === 1) {
          const library = await api.aiDescribe(todo[0].name, todo[0].publisher);
          const current = get().library;
          // 只取回简介这一部分，免得用旧的快照盖掉这期间的其他改动
          if (current) set({ library: { ...current, summaries: library.summaries } });
        } else {
          // 好几个时凑成一批问 AI（后端每次请求放 15 个），不是一个一个问
          const result = await api.aiDescribeMany(todo);
          const current = get().library;
          if (current) set({ library: { ...current, summaries: result.library.summaries } });
          get().notify(
            result.unknown > 0
              ? translate(lang, "ai.describedSome", { n: result.written, failed: result.unknown })
              : translate(lang, "ai.describedMany", { n: result.written }),
          );
        }
      } catch (error) {
        get().notify(errorText(lang, error), "error");
      } finally {
        release();
      }
    },

    refreshLibrary: async () => {
      if (!get().library) return;
      try {
        const library = await api.refreshLibrary();
        if (library) adopt(library);
      } catch (error) {
        get().notify(errorText(language(), error), "error");
      }
    },

    saveSettings: async (patch) => {
      const library = get().library;
      if (!library) return;
      const before = library.settings;
      const settings = { ...before, ...patch };
      // 先改界面再存盘，切主题不用等磁盘
      applyTheme(settings);
      set({ library: { ...library, settings } });
      if (await get().apply(() => api.setSettings(settings))) {
        if (settings.fetchSummaries && (!before.fetchSummaries || before.theSvgIcons !== settings.theSvgIcons)) {
          if (iconFailures >= 3) askedIcons.clear();
          iconFailures = 0;
          queueWebIcons();
        }
        return;
      }
      // 没存上（比如 NAS 断了）：界面退回原来的设置，免得看着改了、下次打开又变回去
      const current = get().library;
      if (current) {
        applyTheme(before);
        set({ library: { ...current, settings: before } });
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

/** 这台电脑上能不能用 AI：设置好了服务和模型，需要密钥的已经存了密钥。 */
export function useAiReady(): boolean {
  // 只订阅一个布尔值：列表里每一行都会用到，索引换了不该让每行都重画
  return useApp((state) => aiUsable(state.library?.settings.ai, state.aiKeySaved));
}

/** 当前语言。仓库还没打开时用上次的语言或系统语言。 */
export function useLanguage(): Language {
  return useApp((state) => state.library?.settings.language) ?? fallbackLanguage();
}

/** 取翻译函数：`const t = useT(); t("common.save")`。 */
export function useT(): Translate {
  const language = useLanguage();
  // 导入语言包后这个数会变，界面跟着重新取一遍文字
  const revision = useApp((state) => state.langRevision);
  // 语言不变时一直是同一个函数，依赖它的 useMemo 才不会每次都重算
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useCallback((key: Key, vars?: Vars) => translate(language, key, vars), [language, revision]);
}
