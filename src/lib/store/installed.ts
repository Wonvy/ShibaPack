import type { StoreApi } from "zustand";
import { api } from "../api";
import { errorText } from "../i18n";
import type { AppState } from "../store";
import type { InstalledApp, Language } from "../types";

export interface InstalledSlice {
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

  /** 打开着的卸载窗口：要卸载（或只是分析）的软件，可以好几个。`analyze` 时默认不勾软件本身，只看占用。 */
  uninstallTarget: { apps: InstalledApp[]; analyze: boolean } | null;
  /** 重新列出已安装的软件，并更新和仓库的对应关系。 */
  loadInstalled: () => Promise<void>;
  /** 已确认卸载完成，立即移除旧的列表项和关联。 */
  removeInstalled: (id: string) => void;
}

export function createInstalledSlice(
  set: StoreApi<AppState>["setState"], get: StoreApi<AppState>["getState"],
  refreshLinks: () => Promise<void>, fillIcons: () => Promise<void>, language: () => Language,
): InstalledSlice {
  let revision = 0;
  return {
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
    setInstalledSelection: (ids, primary) => set({
      selectedInstalledIds: ids,
      selectedInstalledId: primary && ids.includes(primary) ? primary : ids[0] ?? null,
    }),
    uninstallTarget: null,
    removeInstalled: (id) => {
      revision++;
      const state = get();
      const installed = state.installed?.filter((app) => app.id !== id) ?? null;
      const { [id]: removedApp, ...installedIndex } = state.installedIndex;
      const { [id]: removedSize, ...sizes } = state.sizes;
      const selectedInstalledIds = state.selectedInstalledIds.filter((selected) => selected !== id);
      set({
        installed, installedIndex, sizes, selectedInstalledIds,
        selectedInstalledId: state.selectedInstalledId === id ? selectedInstalledIds[0] ?? null : state.selectedInstalledId,
        links: Object.fromEntries(Object.entries(state.links).filter(([, appId]) => appId !== id)),
      });
    },
    loadInstalled: async () => {
      if (get().installedLoading) return;
      const scanRevision = revision;
      set({ installedLoading: true });
      try {
        const repoRevision = get().status?.repoRevision;
        const { apps: installed, cachedIcons } = await api.listInstalled(repoRevision);
        if (scanRevision !== revision) return;
        const library = get().library;
        const stillThere = new Set(installed.map((app) => app.id));
        const selectedInstalledIds = get().selectedInstalledIds.filter((id) => stillThere.has(id));
        const selectedInstalledId = installed.some((app) => app.id === get().selectedInstalledId)
          ? get().selectedInstalledId
          : selectedInstalledIds[0] ?? null;
        set({
          installed,
          installedIndex: Object.fromEntries(installed.map((app) => [app.id, app])),
          selectedInstalledId,
          selectedInstalledIds,
          installedScannedAt: Date.now(),
          ...(cachedIcons && library && get().status?.repoRevision === repoRevision ? { library: { ...library, installedIcons: cachedIcons } } : {}),
        });
        void refreshLinks();
        void fillIcons();
        // 占用空间统计得慢，列表先出来，大小随后补上
        set({ measuring: true });
        void api.measureInstalled().then(
          (sizes) => set({ sizes: Object.fromEntries(Object.entries(sizes).filter(([id]) => id in get().installedIndex)), measuring: false }),
          () => set({ measuring: false }),
        );
      } catch (error) {
        get().notify(errorText(language(), error), "error");
        if (get().installed === null) set({ installed: [] });
      } finally {
        set({ installedLoading: false });
        if (scanRevision !== revision) void get().loadInstalled();
      }
    },

  };
}
