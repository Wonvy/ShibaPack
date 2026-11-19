<script setup lang="ts">
import { computed, type Component } from "vue";
import {
  BookOpen,
  Box,
  Briefcase,
  Code2,
  Cpu,
  FileCode2,
  Folder,
  Gamepad2,
  Globe,
  Heart,
  Image,
  Layers,
  Music,
  Palette,
  Settings2,
  Shield,
  Star,
  Tag,
  Terminal,
  Wrench,
  Zap,
} from "lucide-vue-next";

const props = withDefaults(defineProps<{ name?: string; size?: number }>(), {
  name: "tag",
  size: 14,
});

const icons: Record<string, Component> = {
  wrench: Wrench,
  code: Code2,
  "file-code": FileCode2,
  terminal: Terminal,
  palette: Palette,
  settings: Settings2,
  tag: Tag,
  folder: Folder,
  game: Gamepad2,
  book: BookOpen,
  work: Briefcase,
  music: Music,
  image: Image,
  shield: Shield,
  cpu: Cpu,
  globe: Globe,
  star: Star,
  heart: Heart,
  layers: Layers,
  box: Box,
  zap: Zap,
};

const icon = computed(() => icons[props.name ?? ""] ?? Tag);
</script>

<template>
  <component :is="icon" :size="props.size" :stroke-width="1.75" />
</template>
