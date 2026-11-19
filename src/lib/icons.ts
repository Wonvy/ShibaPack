// 分类可以选的图标，以及各平台的图标。

import {
  BookOpen,
  Box,
  Briefcase,
  Cloud,
  Code,
  Cpu,
  Database,
  Download,
  Film,
  Folder,
  Gamepad2,
  Globe,
  Heart,
  Image,
  type LucideIcon,
  MessageCircle,
  Monitor,
  Music,
  Palette,
  Shield,
  Smartphone,
  Star,
  Terminal,
  Wrench,
} from "lucide-react";
import type { Platform } from "./types";

export const CATEGORY_ICONS: Record<string, LucideIcon> = {
  folder: Folder,
  code: Code,
  terminal: Terminal,
  palette: Palette,
  image: Image,
  briefcase: Briefcase,
  book: BookOpen,
  film: Film,
  music: Music,
  globe: Globe,
  cloud: Cloud,
  download: Download,
  message: MessageCircle,
  gamepad: Gamepad2,
  wrench: Wrench,
  shield: Shield,
  cpu: Cpu,
  database: Database,
  box: Box,
  star: Star,
  heart: Heart,
};

export const CATEGORY_COLORS = ["#5aa2ff", "#a78bfa", "#4cc38a", "#f0a04b", "#f472b6", "#e5646a", "#3fc1c9", "#a4a7ae"];

export function categoryIcon(name: string | null): LucideIcon {
  return (name && CATEGORY_ICONS[name]) || Folder;
}

export const PLATFORM_ICONS: Record<Platform, LucideIcon> = {
  windows: Monitor,
  macos: Monitor,
  linux: Terminal,
  android: Smartphone,
  ios: Smartphone,
  cross: Box,
};
