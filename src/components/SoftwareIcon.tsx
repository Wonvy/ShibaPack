import { FileArchive, Package } from "lucide-react";
import { useState } from "react";
import { assetUrl, isImageDataUrl } from "../lib/api";
import { packageIconKind } from "../lib/library";
import { useApp } from "../lib/store";
import type { InstalledApp, Library, Software } from "../lib/types";

type PackageIconKind = ReturnType<typeof packageIconKind>;

/**
 * 软件图标。`icon` 可以是内嵌图片或文件的绝对路径；没有图标或图标加载失败时，
 * 显示安装包占位图标或软件名字的首字。
 * `stack` 是版本数：多于一个版本时，图标后面露出一两层，像一叠卡片。
 */
export function SoftwareIcon({ name, icon, size = 32, stack = 1, packageKind }: { name: string; icon?: string | null; size?: number; stack?: number; packageKind?: PackageIconKind }) {
  const face = <IconFace name={name} icon={icon} size={size} packageKind={packageKind} />;
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
            background: "var(--line-strong)",
          }}
        />
      ))}
      <span className="relative flex">{face}</span>
    </span>
  );
}

function IconFace({ name, icon, size, packageKind }: { name: string; icon?: string | null; size: number; packageKind?: PackageIconKind }) {
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
  const PackageGlyph = packageKind === "archive" ? FileArchive : packageKind === "installer" ? Package : null;
  return (
    <span
      aria-hidden
      className="flex shrink-0 items-center justify-center bg-tile font-semibold text-tile-fg"
      style={{ width: size, height: size, borderRadius: size * 0.26, fontSize: size * 0.44 }}
    >
      {PackageGlyph ? <PackageGlyph size={Math.round(size * 0.6)} strokeWidth={1.75} /> : first}
    </span>
  );
}

function cachedInstalledIcon(name: string, cache: Library["installedIcons"]): string | null {
  const icon = cache?.[name.trim().toLowerCase()];
  return isImageDataUrl(icon) ? icon : null;
}

function softwareIcon(software: Software, root: string | null | undefined, cache?: Library["installedIcons"]): string | null {
  if (isImageDataUrl(software.iconData)) return software.iconData;
  return cachedInstalledIcon(software.name, cache) ?? (software.icon && root ? `${root}/${software.icon}` : null);
}

export function RepositorySoftwareIcon({ software, size, stack }: { software: Software; size?: number; stack?: number }) {
  const icon = useApp((state) => softwareIcon(software, state.status?.repoRoot, state.library?.installedIcons));
  const first = software.versions.find((version) => version.files.length > 0)?.files[0];
  const kind = first ? packageIconKind(first.name) : undefined;
  const packageKind = kind && software.versions.every((version) => version.files.every((file) => packageIconKind(file.name) === kind))
    ? kind : undefined;
  return <SoftwareIcon name={software.name} icon={icon} size={size} stack={stack} packageKind={packageKind} />;
}

export function InstalledSoftwareIcon({ app, size }: { app: InstalledApp; size?: number }) {
  const icon = useApp((state) => {
    const local = app.icon ?? cachedInstalledIcon(app.name, state.library?.installedIcons);
    if (local) return local;
    const software = state.library?.software.find((software) => state.links[software.id] === app.id);
    return software ? softwareIcon(software, state.status?.repoRoot) : null;
  });
  return <SoftwareIcon name={app.name} icon={icon} size={size} />;
}
