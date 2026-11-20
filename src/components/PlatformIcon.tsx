// 平台图标：Windows、macOS、Linux、Android、iOS、跨平台。单色，跟随文字颜色。

import { File, FileArchive, Layers, Package } from "lucide-react";
import { siAndroid, siApple, siIos, siLinux } from "simple-icons";
import { packageIconKind } from "../lib/library";
import { useT } from "../lib/store";
import type { Platform } from "../lib/types";
import { useHoverCard } from "./HoverCard";
import { cx } from "./ui";

// 图标库里没有 Windows 的标志（微软不允许收录），这里用四格窗的图形
const WINDOWS = "M0 0h11.3v11.3H0zM12.7 0H24v11.3H12.7zM0 12.7h11.3V24H0zM12.7 12.7H24V24H12.7z";

const PATHS: Record<Exclude<Platform, "cross">, string> = {
  windows: WINDOWS,
  macos: siApple.path,
  linux: siLinux.path,
  android: siAndroid.path,
  ios: siIos.path,
};

export function PlatformIcon({ platform, size = 14 }: { platform: Platform; size?: number }) {
  const t = useT();
  const label = t(`platform.${platform}`);
  const hover = useHoverCard(label, "label");
  return (
    <span role="img" aria-label={label} className="inline-flex shrink-0 text-fg-4 transition-colors hover:text-fg-3 motion-reduce:transition-none" {...hover.handlers}>
      {platform === "cross" ? <Layers size={size} strokeWidth={1.75} aria-hidden /> : (
        <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden>
          <path d={PATHS[platform]} />
        </svg>
      )}
      {hover.card}
    </span>
  );
}

export function PackageFileIcon({ name, platform, size = 14, className }: { name: string; platform?: Platform; size?: number; className?: string }) {
  const kind = packageIconKind(name);
  if (!kind && platform) return <PlatformIcon platform={platform} size={size} />;
  const Icon = kind === "archive" ? FileArchive : kind === "installer" ? Package : File;
  return <Icon size={size} strokeWidth={1.75} aria-hidden className={cx("shrink-0", className)} />;
}

/** 一排平台图标，鼠标停上去显示平台名。 */
export function PlatformIcons({ platforms, size = 14, className }: { platforms: Platform[]; size?: number; className?: string }) {
  return (
    <span className={cx("flex items-center gap-2", className)}>
      {platforms.map((platform) => <PlatformIcon key={platform} platform={platform} size={size} />)}
    </span>
  );
}
