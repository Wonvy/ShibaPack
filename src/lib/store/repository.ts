import type { StoreApi } from "zustand";
import { api } from "../api";
import { errorText } from "../i18n";
import { needsTranslation } from "../library";
import type { AppState } from "../store";
import type { Language, Library } from "../types";

const ICON_WORKERS = 3;
const SUMMARY_WORKERS = 3;

/** 每次打开仓库都使用独立队列，旧任务的结果不再交给新界面。 */
export function createRepositoryTasks(
  revision: number, set: StoreApi<AppState>["setState"], get: StoreApi<AppState>["getState"], language: () => Language,
) {
  const active = () => get().status?.repoRevision === revision;
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
  const askedSummaries = new Set<string>();
  const summaryQueue: { name: string; refresh: boolean }[] = [];
  let summaryWorkers = 0;
  let summariesOffline = false;

  const refreshLinks = async () => {
    if (get().installed === null || get().library === null) return;
    try {
      const links = await api.matchInstalled(revision);
      if (!active()) return;
      set({ links: Object.fromEntries(Object.entries(links).filter(([, id]) => id in get().installedIndex)) });
    } catch {
      // 对应关系只是锦上添花，取不到就保持原样
    }
  };

  /** 给没有图标的软件补图标。后端一次处理几个，这里一直调到没有剩余为止。 */
  const fillIcons = async () => {
    // 等已安装软件列出来再开始：图标可能要从那边借
    if (!active() || fillingIcons || get().installed === null || get().library === null) return;
    fillingIcons = true;
    try {
      for (let round = 0; round < 500 && active(); round++) {
        const result = await api.fillIcons(revision);
        if (!active()) return;
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
    if (!active() || fillingInfo) return;
    fillingInfo = true;
    try {
      for (let round = 0; round < 500 && active(); round++) {
        const result = await api.fillFileInfo(revision);
        if (!active()) return;
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
    const settings = get().library?.settings;
    if (!active() || !settings?.fetchSummaries) return;
    for (const id of iconQueue) {
      if (iconWorkers >= ICON_WORKERS || iconFailures >= 3) break;
      iconQueue.delete(id);
      iconWorkers++;
      api
        .fetchIcon(id, revision)
        .then((update) => {
          if (!active()) return;
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
          if (active() && current.library?.settings.fetchSummaries
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
    while (active() && summaryWorkers < SUMMARY_WORKERS && summaryQueue.length > 0 && !summariesOffline) {
      const { name, refresh } = summaryQueue.shift()!;
      summaryWorkers++;
      api
        .fetchSummary(name, refresh, revision)
        .then((library) => {
          if (!active()) return;
          const current = get().library;
          // 只取回简介这一部分，免得用旧的快照盖掉这期间的其他改动
          if (current) set({ library: { ...current, summaries: library.summaries } });
        })
        .catch((error) => {
          // 网络不通：这次运行不再自动尝试，下次启动再说（手动点“获取简介”还可以再试）
          if (active() && String(error).startsWith("offline")) {
            summariesOffline = true;
            const dropped = new Set(summaryQueue.splice(0).map((item) => item.name));
            set({ summaryBusy: Object.fromEntries(Object.entries(get().summaryBusy).filter(([key]) => !dropped.has(key))) });
            if (refresh) get().notify(errorText(language(), error), "error");
          }
        })
        .finally(() => {
          summaryWorkers--;
          if (!active()) return;
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
    if (!active() || !library.settings.fetchSummaries || summariesOffline) return;
    const before = summaryQueue.length;
    const busy = { ...get().summaryBusy };
    for (const software of library.software) {
      const key = summaryWanted(library, software.name);
      if (key === null || askedSummaries.has(key)) continue;
      askedSummaries.add(key);
      if (busy[software.name]) continue;
      summaryQueue.push({ name: software.name, refresh: false });
      busy[software.name] = true;
    }
    if (summaryQueue.length !== before) set({ summaryBusy: busy });
    pumpSummaries();
  };

  const requestSummary = (name: string, refresh = false) => {
    const library = get().library;
    if (!active() || !library || !name.trim() || get().summaryBusy[name]) return;
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
  };

  const retryWebIcons = () => {
    if (iconFailures >= 3) askedIcons.clear();
    iconFailures = 0;
    queueWebIcons();
  };

  return { revision, refreshLinks, fillIcons, fillFileInfo, queueWebIcons, queueSummaries, requestSummary, retryWebIcons };
}
