// 启动项和服务的图标：认出所属应用时用应用的图标，否则按类型给一个火箭（启动项）或齿轮（服务）。

import { Cog, type LucideIcon, Rocket } from "lucide-react";
import type { StartupItem } from "../lib/types";
import { SoftwareIcon } from "./SoftwareIcon";

export function StartupIcon({ item, size }: { item: StartupItem; size: number }) {
  if (item.icon) return <SoftwareIcon name={item.app || item.name} icon={item.icon} size={size} />;
  const Icon: LucideIcon = item.kind === "service" ? Cog : Rocket;
  return (
    <span
      aria-hidden
      className="flex shrink-0 items-center justify-center bg-tile text-tile-fg"
      style={{ width: size, height: size, borderRadius: size * 0.26 }}
    >
      <Icon size={Math.round(size * 0.55)} strokeWidth={1.75} />
    </span>
  );
}
