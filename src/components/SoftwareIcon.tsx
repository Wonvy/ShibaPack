import { useState } from "react";
import { assetUrl } from "../lib/api";
import { colorFor } from "../lib/library";
import { useApp } from "../lib/store";
import type { Software } from "../lib/types";

/**
 * 软件图标。`icon` 是图标文件的绝对路径；没有图标或图标加载失败时，
 * 用名字的首字加一块固定的底色。
 */
export function SoftwareIcon({ name, icon, size = 32 }: { name: string; icon?: string | null; size?: number }) {
  const [broken, setBroken] = useState<string | null>(null);
  if (icon && broken !== icon) {
    return (
      <img
        src={assetUrl(icon)}
        alt=""
        draggable={false}
        onError={() => setBroken(icon)}
        className="shrink-0 object-contain"
        style={{ width: size, height: size }}
      />
    );
  }
  const first = [...name.trim()][0]?.toUpperCase() ?? "?";
  return (
    <span
      aria-hidden
      className="flex shrink-0 items-center justify-center font-semibold text-white"
      style={{ width: size, height: size, borderRadius: size * 0.26, fontSize: size * 0.44, background: colorFor(name) }}
    >
      {first}
    </span>
  );
}

/** 仓库里一个软件的图标文件路径（索引里存的是相对仓库根目录的路径）。 */
export function useSoftwareIcon(software: Software): string | null {
  const root = useApp((state) => state.status?.repoRoot);
  return software.icon && root ? `${root}/${software.icon}` : null;
}
