// 应用状态：仓库状态、索引数据、已安装软件、当前页面、筛选条件、导入进度、提示。

import { useCallback } from "react";
import { create } from "zustand";
import { aiUsable } from "./ai";
import { api, onCheckProgress, onDownloadProgress, onImportProgress, pickFolder } from "./api";
import { type Key, type Translate, type Vars, errorText, setLangPacks, translate } from "./i18n";
import { filterSoftware, hasUpdate, needsTranslation } from "./library";
import { applyTheme } from "./theme";
import type {
  Asset,
  CheckProgress,
  DropEntry,
  ImportHint,
  ImportReport,
  InstalledApp,
  LangPack,
  Language,
  Library,
  Platform,
  Progress,
  Settings,
  SourceKind,
  Status,
  Storage,
} from "./types";

export type Route = "home" | "library" | "installed" | "startup" | "cleanup" | "disk";
const ROUTES: Route[] = ["home", "library", "installed", "startup", "cleanup", "disk"];

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

/** 一个正在下载或排队的文件。 */
export interface DownloadState {
  softwareId: string;
  name: string;
  received: number;
  total: number | null;
}

/** 导入窗口：拖进中间栏后先在这里选分类、存储方式，确认后导入并显示进度和结果。 */
export interface ImportDialogState {
  paths: string[];
  /** 已经看过的拖入项（从侧栏拖进来时先看过了）。 */
  entries?: DropEntry[];
  /** 预先选好的分类（拖到侧栏的某个分类上；`null` 是拖到了“未分类”上）。 */
  categoryId?: string | null;
  storage?: Storage;
  softwareName?: string;
  /** 不是从这个窗口开始的导入有失败：直接显示结果和失败原因。 */
  report?: ImportReport;
}

export interface ImportOptions {
  /** 移动进仓库（不留原文件）。不给时按设置来。 */
  move?: boolean;
  /** 不弹提示、不跳到导入的软件、失败时也不开结果窗口（导入窗口自己显示结果）。 */
  quiet?: boolean;
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
  /** 选中的软件；多选时是最先选的那个（Shift 连选从它算起）。 */
  selectedId: string | null;
  /** 多选时所有选中的软件；单选时只有 selectedId 一个。 */
  selectedIds: string[];
  progress: Progress | null;
  /** 导入进行中时又拖进来、选进来的文件，排着队等前一批导完。 */
  importQueue: { paths: string[]; mode?: Storage; hint?: ImportHint; options?: ImportOptions; done: (report: ImportReport | null) => void }[];
  dropping: boolean;
  /** 从系统拖文件进来时，指针下面可以落下的地方（侧栏的分类：`category:<id>`、`uncategorized`）。 */
  fileDropTarget: string | null;
  /** 打开着的导入窗口。 */
  importDialog: ImportDialogState | null;
  /** 屏幕下方的提示，最新的在最后，最多同时 3 条。新提示不会把还没看完的出错提示挤掉。 */
  toasts: Toast[];
  dismissToast: (id: number) => void;
  settingsOpen: boolean;

  /** 这台电脑上已安装的软件。还没列出来时是 null。 */
  installed: InstalledApp[] | null;
  /** 已安装软件按 id 查：列表里每一行都要查“它装着没有”，不能每次都从头找一遍。 */
  installedIndex: Record<string, InstalledApp>;
  installedLoading: boolean;
  /** 上次列完已安装软件的时间（毫秒），用来决定窗口回到前台时要不要重新扫描。 */
  installedScannedAt: number;
  /** 已安装软件占用的空间，统计完才有。 */
  sizes: Record<string, number>;
  /** 正在统计已安装软件占用的空间。统计失败也会停下，界面不会一直显示“统计中”。 */
  measuring: boolean;
  /** 仓库里的软件 id → 对应的已安装软件 id。 */
  links: Record<string, string>;
  selectedInstalledId: string | null;
  /** 已安装软件里多选的；单选时只有 selectedInstalledId 一个。 */
  selectedInstalledIds: string[];
  /** 单选一个已安装软件（null 为不选）。 */
  selectInstalled: (id: string | null) => void;
  /** 多选已安装软件：`primary` 是 Shift 连选的起点。 */
  setInstalledSelection: (ids: string[], primary: string | null) => void;

  /** 正在下载或排队的文件，按下载地址。 */
  downloads: Record<string, DownloadState>;
  /** 正在检查更新时的进度。 */
  checking: CheckProgress | null;
  /** 打开着的智能分类预览：要分类的软件。 */
  classifyIds: string[] | null;
  /** 打开着的卸载窗口：要卸载（或只是分析）的软件，可以好几个。`analyze` 时默认不勾软件本身，只看占用。 */
  uninstallTarget: { apps: InstalledApp[]; analyze: boolean } | null;
  /**
   * 打开着的“关联在线源”对话框：给哪个软件关联；null 表示从在线源新添一个软件。
   * 从顶栏搜索的在线结果打开时，带着源、标识和名字，对话框直接查出最新版本等用户确认。
   */
  sourceDialog: { softwareId: string | null; kind?: SourceKind; reference?: string; name?: string } | null;
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
  /** `hint.name` 给了时，所有文件都归到这个软件下（在详情栏里给某个软件选文件添加版本）。从系统拖进来的文件不带它，一律按文件名认。 */
  importPaths: (paths: string[], mode?: Storage, hint?: ImportHint, options?: ImportOptions) => Promise<ImportReport | null>;
  /** 停下正在进行的导入（在两个文件之间停），排着队的也不导了。 */
  cancelImport: () => void;
  importCancelled: boolean;
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
let importing = false;
const summaryQueue: { name: string; refresh: boolean }[] = [];
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
    applyTheme(library.settings.theme, library.settings.accent);
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
    progress: null,
    importQueue: [],
    importCancelled: false,
    summaryBusy: {},
    aiKeySaved: false,
    aiBusy: {},
    dropping: false,
    fileDropTarget: null,
    importDialog: null,
    toasts: [],
    settingsOpen: false,
    installed: null,
    installedIndex: {},
    installedLoading: false,
    installedScannedAt: 0,
    sizes: {},
    measuring: false,
    links: {},
    selectedInstalledId: null,
    selectedInstalledIds: [],
    selectInstalled: (id) => set({ selectedInstalledId: id, selectedInstalledIds: id ? [id] : [] }),
    setInstalledSelection: (ids, primary) => set({ selectedInstalledIds: ids, selectedInstalledId: primary ?? ids[0] ?? null }),
    downloads: {},
    checking: null,
    classifyIds: null,
    uninstallTarget: null,
    sourceDialog: null,
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

    importPaths: async (paths, mode, hint, options = {}) => {
      const library = get().library;
      if (!library || paths.length === 0) return null;
      // 正在导入：排队，等前一批导完再导，不能悄悄丢掉
      if (importing) {
        if (!options.quiet) get().notify(translate(language(), "import.queued"));
        return new Promise((done) => set({ importQueue: [...get().importQueue, { paths, mode, hint, options, done }] }));
      }
      importing = true;
      set({ progress: { phase: "collect", current: 0, total: 0, file: "" } });
      let report: ImportReport | null = null;
      try {
        // 没指定存储模式时按设置来；“移动进仓库”也只在按设置导入时生效（扫描建索引明确要求只记位置）
        const { defaultStorage, moveOnImport } = library.settings;
        const move = options.move ?? (!mode && defaultStorage === "managed" && moveOnImport);
        const result = await api.importPaths(paths, mode ?? defaultStorage, hint, move);
        adopt(result.library);
        report = result.report;
        const { imported, duplicates, failed, items, cancelled } = report;
        if (options.quiet) {
          // 导入窗口自己显示结果
        } else if (failed > 0) {
          // 有失败的：打开结果窗口，一个个说清楚哪个文件、在哪一步、为什么
          set({ importDialog: { paths, report } });
        } else if (cancelled) {
          get().notify(translate(language(), "import.cancelled", { imported, duplicates, failed }));
        } else if (items.length === 0) {
          get().notify(translate(language(), "import.none"));
        } else {
          get().notify(translate(language(), "import.done", { imported, duplicates, failed }));
          // 只导入了一个软件时直接选中它
          const ids = new Set(items.filter((i) => i.status === "imported").map((i) => i.softwareId));
          if (ids.size === 1) {
            get().setRoute("library");
            get().select([...ids][0]);
          }
        }
      } catch (error) {
        if (!options.quiet) get().notify(errorText(language(), error), "error");
        else throw error;
      } finally {
        importing = false;
        set({ progress: null });
        // 取消时排着的也不导了
        const [next, ...rest] = get().importQueue;
        if (next && !get().importCancelled) {
          set({ importQueue: rest });
          void get().importPaths(next.paths, next.mode, next.hint, next.options).then(next.done, () => next.done(null));
        } else {
          for (const waiting of get().importQueue) waiting.done(null);
          set({ importQueue: [], importCancelled: false });
        }
      }
      return report;
    },

    cancelImport: () => {
      set({ importCancelled: true });
      void api.cancelImport();
    },

    saveSettings: async (patch) => {
      const library = get().library;
      if (!library) return;
      const before = library.settings;
      const settings = { ...before, ...patch };
      // 先改界面再存盘，切主题不用等磁盘
      applyTheme(settings.theme, settings.accent);
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
        applyTheme(before.theme, before.accent);
        set({ library: { ...current, settings: before } });
      }
    },

    loadInstalled: async () => {
      if (get().installedLoading) return;
      set({ installedLoading: true });
      try {
        const root = get().status?.repoRoot;
        const { apps: installed, cachedIcons } = await api.listInstalled();
        const library = get().library;
        const selectedInstalledId = installed.some((app) => app.id === get().selectedInstalledId)
          ? get().selectedInstalledId
          : null;
        const stillThere = new Set(installed.map((app) => app.id));
        set({
          installed,
          installedIndex: Object.fromEntries(installed.map((app) => [app.id, app])),
          selectedInstalledId,
          selectedInstalledIds: get().selectedInstalledIds.filter((id) => stillThere.has(id)),
          installedScannedAt: Date.now(),
          ...(cachedIcons && library && get().status?.repoRoot === root ? { library: { ...library, installedIcons: cachedIcons } } : {}),
        });
        void refreshLinks();
        void fillIcons();
        // 占用空间统计得慢，列表先出来，大小随后补上
        set({ measuring: true });
        void api.measureInstalled().then(
          (sizes) => set({ sizes, measuring: false }),
          () => set({ measuring: false }),
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
