// 从系统拖文件进窗口时的落点：侧栏的分类可以直接放进去，别处打开导入窗口。

import { api, type DropPoint } from "./api";
import { useApp } from "./store";

/** 指针下面能直接放进去的分类：`category:<id>`、`uncategorized`；别处返回 null。 */
export function fileDropTargetAt(point: DropPoint): string | null {
  const raw = document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>("[data-drop]")?.dataset.drop ?? null;
  return raw && (raw === "uncategorized" || raw.startsWith("category:")) ? raw : null;
}

/**
 * 拖到侧栏的某个分类上（`null` 是“未分类”）：只有文件和 .app 时直接导入进这个分类；
 * 带着普通文件夹时打开导入窗口，分类已经选好，让用户决定文件夹是整个导入还是导入里面的安装包。
 */
export async function dropOnCategory(paths: string[], categoryId: string | null) {
  const app = useApp.getState();
  const entries = await api.inspectDrop(paths).catch(() => null);
  const folders = entries?.filter((entry) => entry.isDir) ?? [];
  if (entries === null || folders.some((entry) => !entry.name.toLowerCase().endsWith(".app"))) {
    app.patch({ importDialog: { paths, entries: entries ?? undefined, categoryId } });
    return;
  }
  await app.importPaths(paths, undefined, { categoryId: categoryId ?? undefined, autoClassify: false, packages: folders.map((entry) => entry.path) });
}
