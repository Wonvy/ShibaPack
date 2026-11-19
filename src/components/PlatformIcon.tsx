// 平台图标：Windows、macOS、Linux、Android、iOS、跨平台。单色，跟随文字颜色。

import { Layers } from "lucide-react";
import { siAndroid, siApple, siIos, siLinux } from "simple-icons";
import { useT } from "../lib/store";
import type { Platform } from "../lib/types";
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

export function PlatformIcon({ platform, size = 14, className }: { platform: Platform; size?: number; className?: string }) {
  if (platform === "cross") return <Layers size={size} strokeWidth={1.75} aria-hidden className={cx("shrink-0", className)} />;
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden className={cx("shrink-0", className)}>
      <path d={PATHS[platform]} />
    </svg>
  );
}

/** 一排平台图标，鼠标停上去显示平台名。 */
export function PlatformIcons({ platforms, size = 14, className }: { platforms: Platform[]; size?: number; className?: string }) {
  const t = useT();
  return (
    <span className={cx("flex items-center gap-2 text-fg-3", className)}>
      {platforms.map((platform) => (
        <span key={platform} title={t(`platform.${platform}`)} aria-label={t(`platform.${platform}`)} role="img" className="flex">
          <PlatformIcon platform={platform} size={size} />
        </span>
      ))}
    </span>
  );
}

/** 灰色小标签里放图标和平台名，用在位置宽裕的地方（详情栏的文件行）。 */
export function PlatformChip({ platform }: { platform: Platform }) {
  const t = useT();
  return (
    <span className="inline-flex h-5 shrink-0 items-center gap-1 rounded-[4px] bg-hover px-1.5 text-[12px] text-fg-3">
      <PlatformIcon platform={platform} size={12} />
      {t(`platform.${platform}`)}
    </span>
  );
}
