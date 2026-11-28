import type { StoreApi } from "zustand";
import { api } from "../api";
import { errorText, translate } from "../i18n";
import type { AppState } from "../store";
import type { DropEntry, ImportHint, ImportReport, Language, Library, Progress, Storage } from "../types";

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

export interface ImportSlice {
  progress: Progress | null;
  /** 导入进行中时又拖进来、选进来的文件，排着队等前一批导完。 */
  importQueue: { paths: string[]; mode?: Storage; hint?: ImportHint; options?: ImportOptions; done: (report: ImportReport | null) => void }[];
  dropping: boolean;
  /** 从系统拖文件进来时，指针下面可以落下的地方（侧栏的分类：`category:<id>`、`uncategorized`）。 */
  fileDropTarget: string | null;
  /** 打开着的导入窗口。 */
  importDialog: ImportDialogState | null;
  /** `hint.name` 给了时，所有文件都归到这个软件下（在详情栏里给某个软件选文件添加版本）。从系统拖进来的文件不带它，一律按文件名认。 */
  importPaths: (paths: string[], mode?: Storage, hint?: ImportHint, options?: ImportOptions) => Promise<ImportReport | null>;
  /** 停下正在进行的导入，排着队的也不导了。 */
  cancelImport: () => void;
  importCancelled: boolean;
  /** 把仓库根目录里散落的安装包归位。 */
  tidyRoot: () => Promise<void>;
}

export function createImportSlice(
  set: StoreApi<AppState>["setState"], get: StoreApi<AppState>["getState"],
  adopt: (library: Library) => void, language: () => Language,
): ImportSlice {
  let importing = false;
  return {
    progress: null,
    importQueue: [],
    importCancelled: false,
    dropping: false,
    fileDropTarget: null,
    importDialog: null,
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

  };
}
