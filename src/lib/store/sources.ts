import type { StoreApi } from "zustand";
import { api } from "../api";
import { type Key, errorText, translate } from "../i18n";
import { hasUpdate } from "../library";
import type { AppState } from "../store";
import type { Asset, CheckProgress, Language, Library, SourceKind } from "../types";

/** 一个正在下载或排队的文件。 */
export interface DownloadState {
  softwareId: string;
  name: string;
  received: number;
  total: number | null;
}

export interface SourceSlice {
  /** 正在下载或排队的文件，按下载地址。 */
  downloads: Record<string, DownloadState>;
  /** 正在检查更新时的进度。 */
  checking: CheckProgress | null;
  /**
   * 打开着的“关联在线源”对话框：给哪个软件关联；null 表示从在线源新添一个软件。
   * 从顶栏搜索的在线结果打开时，带着源、标识和名字，对话框直接查出最新版本等用户确认。
   */
  sourceDialog: { softwareId: string | null; kind?: SourceKind; reference?: string; name?: string } | null;
  /** 检查所有关联了在线源的软件有没有新版本。 */
  checkUpdates: () => Promise<void>;
  /** 下载在线源上的一个安装包并收进仓库。 */
  downloadAsset: (softwareId: string, asset: Asset) => Promise<void>;
}

export function createSourceSlice(
  set: StoreApi<AppState>["setState"], get: StoreApi<AppState>["getState"],
  adopt: (library: Library) => void, language: () => Language,
): SourceSlice {
  return {
    downloads: {},
    checking: null,
    sourceDialog: null,
    checkUpdates: async () => {
      const library = get().library;
      if (!library || get().checking) return;
      if (!library.software.some((s) => s.subscription)) {
        get().notify(translate(language(), "updates.none"));
        return;
      }
      set({ checking: { current: 0, total: 1, name: "" } });
      const revision = get().status?.repoRevision;
      try {
        const result = await api.checkUpdates(revision);
        if (get().status?.repoRevision !== revision) return;
        adopt(result.library);
        const updates = result.library.software.filter(hasUpdate).length;
        const key: Key = result.rateLimited ? "updates.rateLimited" : result.failed > 0 ? "updates.failed" : "updates.done";
        get().notify(translate(language(), key, { checked: result.checked, updates, failed: result.failed }), result.rateLimited ? "error" : "info");
      } catch (error) {
        if (get().status?.repoRevision === revision) get().notify(errorText(language(), error), "error");
      } finally {
        if (get().status?.repoRevision === revision) set({ checking: null });
      }
    },

    downloadAsset: async (softwareId, asset) => {
      if (asset.url in get().downloads) return;
      const revision = get().status?.repoRevision;
      // 先占个位：前面有别的文件在下的话，它会排队
      set({ downloads: { ...get().downloads, [asset.url]: { softwareId, name: asset.name, received: 0, total: asset.size } } });
      try {
        const library = await api.downloadAsset(softwareId, asset.url, revision);
        if (get().status?.repoRevision !== revision) return;
        adopt(library);
        get().notify(translate(language(), "download.done", { name: asset.name }));
      } catch (error) {
        // 自己点的取消不用再提示
        if (get().status?.repoRevision === revision && !String(error).startsWith("cancelled")) get().notify(errorText(language(), error), "error");
      } finally {
        if (get().status?.repoRevision === revision) {
          const downloads = { ...get().downloads };
          delete downloads[asset.url];
          set({ downloads });
        }
      }
    },

  };
}
