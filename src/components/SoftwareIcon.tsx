import { useState } from "react";
import { assetUrl } from "../lib/api";
import { colorFor } from "../lib/library";
import { useApp } from "../lib/store";
import type { Software } from "../lib/types";

/**
 * 软件图标。`icon` 是图标文件的绝对路径；没有图标或图标加载失败时，
 * 用名字的首字加一块固定的底色。
 * `stack` 是版本数：多于一个版本时，图标后面露出一两层，像一叠卡片。
 */
export function SoftwareIcon({ name, icon, size = 32, stack = 1 }: { name: string; icon?: string | null; size?: number; stack?: number }) {
  const face = <IconFace name={name} icon={icon} size={size} />;
  const layers = Math.min(Math.max(stack, 1), 3) - 1;
  if (layers === 0) return face;
  const radius = size * 0.26;
  // 后面的几层往上错开，越往后越窄、越淡
  const step = Math.max(2, Math.round(size / 11));
  return (
    <span className="relative flex shrink-0" style={{ width: size, height: size }}>
      {Array.from({ length: layers }, (_, i) => layers - i).map((depth) => (
        <span
          key={depth}
          aria-hidden
          className="absolute"
          style={{
            top: -step * depth,
            left: step * depth * 1.25,
            right: step * depth * 1.25,
            height: size,
            borderRadius: radius,
            background: icon ? "var(--line-strong)" : colorFor(name),
            opacity: icon ? 1 : depth === 1 ? 0.45 : 0.22,
          }}
        />
      ))}
      <span className="relative flex">{face}</span>
    </span>
  );
}

function IconFace({ name, icon, size }: { name: string; icon?: string | null; size: number }) {
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
