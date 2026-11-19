<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  AppWindow,
  ChevronRight,
  CornerUpLeft,
  HardDrive,
  LayoutGrid,
  Plus,
  Settings2,
  Tag,
  Trash2,
  Wrench,
} from "lucide-vue-next";
import { t } from "../i18n";
import { useAppStore } from "../stores/app";
import { useUiStore } from "../stores/ui";
import type { Category } from "../types";
import CategoryEditor from "./CategoryEditor.vue";
import CategoryIcon from "./CategoryIcon.vue";
import ContextMenu, { type MenuItem } from "./ContextMenu.vue";

const app = useAppStore();
const ui = useUiStore();
const router = useRouter();
const route = useRoute();

const expanded = ref<Set<string>>(new Set());
const menu = ref<{ x: number; y: number; category: Category } | null>(null);
const editor = ref<{ category: Category | null; parentId?: string | null } | null>(null);
const dragId = ref<string | null>(null);
const dragOverId = ref<string | null>(null);

const roots = computed(() =>
  app.library.categories
    .filter((item) => !item.parent_id)
    .sort((a, b) => a.sort_order - b.sort_order),
);

watch(
  roots,
  (list) => {
    if (expanded.value.size > 0) return;
    const parent = list.find((item) => childrenOf(item.id).length > 0);
    if (parent) expanded.value = new Set([parent.id]);
  },
  { immediate: true },
);

function childrenOf(id: string): Category[] {
  return app.library.categories
    .filter((item) => item.parent_id === id)
    .sort((a, b) => a.sort_order - b.sort_order);
}

function countOf(category: Category): number {
  const children = childrenOf(category.id);
  if (children.length === 0) return category.package_count;
  return category.package_count + children.reduce((sum, child) => sum + child.package_count, 0);
}

function isActive(id: string) {
  return route.name === "library" && app.scope.type === "category" && app.scope.id === id;
}

function isAllActive() {
  return route.name === "library" && app.scope.type === "all";
}

function chooseAll() {
  app.chooseScope({ type: "all" });
  app.platformFilter = "all";
  if (route.name !== "library") void router.push("/library");
}

function toggle(id: string) {
  const next = new Set(expanded.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expanded.value = next;
}

function choose(category: Category) {
  if (isActive(category.id) && route.name === "library") {
    app.chooseScope({ type: "all" });
    return;
  }
  app.chooseScope({ type: "category", id: category.id });
  app.platformFilter = "all";
  if (route.name !== "library") void router.push("/library");
}

function openEditor(category: Category | null = null, parentId?: string | null) {
  editor.value = { category, parentId };
}

function openMenu(event: MouseEvent, category: Category) {
  menu.value = { x: event.clientX, y: event.clientY, category };
}

const menuItems = computed<MenuItem[]>(() => {
  const category = menu.value?.category;
  if (!category) return [];
  const items: MenuItem[] = [
    { key: "edit", label: t("sidebar.context.edit"), icon: Settings2 },
    { key: "child", label: t("sidebar.context.newChild"), icon: Plus },
  ];
  if (category.parent_id) {
    items.push({ key: "root", label: t("sidebar.context.moveToRoot"), icon: CornerUpLeft });
  }
  items.push(
    { key: "sep", label: "", separator: true },
    { key: "delete", label: t("sidebar.context.delete"), icon: Trash2, danger: true },
  );
  return items;
});

function onMenuSelect(key: string) {
  const category = menu.value?.category;
  if (!category) return;
  if (key === "edit") {
    openEditor(category);
  } else if (key === "child") {
    openEditor(null, category.id);
  } else if (key === "root") {
    void app.updateCategory({ ...category, parent_id: null });
  } else if (key === "delete") {
    if (window.confirm(t("sidebar.confirmDelete", { name: category.name }))) {
      void app.deleteCategory(category.id);
    }
  }
}

function onDragStart(event: DragEvent, category: Category) {
  dragId.value = category.id;
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", category.id);
  }
}

function sameLevel(a: Category | undefined, b: Category): boolean {
  return Boolean(a && a.parent_id === b.parent_id);
}

function onDragOver(event: DragEvent, category: Category) {
  const source = app.library.categories.find((item) => item.id === dragId.value);
  if (!dragId.value || dragId.value === category.id || !sameLevel(source, category)) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dragOverId.value = category.id;
}

async function onDrop(event: DragEvent, category: Category) {
  event.preventDefault();
  const source = app.library.categories.find((item) => item.id === dragId.value);
  if (!source || !sameLevel(source, category)) return clearDrag();

  const siblings = app.library.categories
    .filter((item) => item.parent_id === category.parent_id)
    .sort((a, b) => a.sort_order - b.sort_order);
  const from = siblings.findIndex((item) => item.id === source.id);
  const to = siblings.findIndex((item) => item.id === category.id);
  if (from >= 0 && to >= 0) {
    const [moved] = siblings.splice(from, 1);
    siblings.splice(to, 0, moved);
    await app.reorderCategories(siblings.map((item) => item.id));
  }
  clearDrag();
}

function clearDrag() {
  dragId.value = null;
  dragOverId.value = null;
}
</script>

<template>
  <aside
    class="flex h-full shrink-0 flex-col border-r border-line bg-sidebar"
    :style="{ width: `${ui.sidebarWidth}px` }"
  >
    <div class="shrink-0 px-2 pb-1 pt-2">
      <button
        type="button"
        class="mb-0.5 flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px] transition-colors"
        :class="route.name === 'installed' ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
        @click="router.push('/installed')"
      >
        <AppWindow :size="14" :stroke-width="1.75" class="shrink-0" />
        <span class="min-w-0 flex-1 truncate">{{ t("sidebar.installed") }}</span>
        <Wrench :size="12" :stroke-width="1.75" class="shrink-0 text-fg-4" />
      </button>
    </div>

    <nav class="min-h-0 flex-1 overflow-y-auto px-2 pb-3 pt-0">
      <div
        class="group mb-0.5 flex h-8 items-center rounded-md transition-colors"
        :class="isAllActive() ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
      >
        <button type="button" class="flex h-8 min-w-0 flex-1 items-center gap-1.5 px-2 text-left text-[12px]" @click="chooseAll">
          <LayoutGrid :size="14" :stroke-width="1.75" class="shrink-0 text-fg-3" />
          <span class="min-w-0 flex-1 truncate">{{ t("sidebar.all") }}</span>
          <span class="tabular text-[11px] text-fg-3">{{ app.library.stats.software_count }}</span>
        </button>
        <button
          type="button"
          class="mr-1 inline-flex size-6 shrink-0 items-center justify-center rounded text-fg-4 opacity-0 transition-opacity hover:bg-active hover:text-fg group-hover:opacity-100"
          :title="t('category.new')"
          :aria-label="t('category.new')"
          @click="openEditor()"
        >
          <Plus :size="13" :stroke-width="1.75" />
        </button>
      </div>

      <template v-for="category in roots" :key="category.id">
        <div
          class="group flex h-8 items-center rounded-md transition-colors"
          :class="[
            isActive(category.id) ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg',
            dragId === category.id ? 'opacity-40' : '',
            dragOverId === category.id ? 'ring-1 ring-accent' : '',
          ]"
          draggable="true"
          @dragstart="onDragStart($event, category)"
          @dragover="onDragOver($event, category)"
          @drop="onDrop($event, category)"
          @dragend="clearDrag"
          @contextmenu.prevent="openMenu($event, category)"
          @dblclick="openEditor(category)"
        >
          <button
            v-if="childrenOf(category.id).length > 0"
            type="button"
            class="flex size-6 shrink-0 items-center justify-center text-fg-4 hover:text-fg-2"
            :aria-label="expanded.has(category.id) ? t('sidebar.collapse') : t('sidebar.expand')"
            @click="toggle(category.id)"
          >
            <ChevronRight
              :size="12"
              :stroke-width="2.25"
              class="transition-transform"
              :class="expanded.has(category.id) ? 'rotate-90' : ''"
            />
          </button>
          <span v-else class="size-6 shrink-0" />

          <button type="button" class="flex min-w-0 flex-1 items-center gap-1.5 pr-2 text-left text-[13px]" @click="choose(category)">
            <span :style="{ color: category.color }"><CategoryIcon :name="category.icon" :size="14" /></span>
            <span class="min-w-0 flex-1 truncate">{{ category.name }}</span>
            <span class="tabular text-[11px] text-fg-3">{{ countOf(category) }}</span>
          </button>
        </div>

        <div v-if="expanded.has(category.id)" class="ml-3 border-l border-line pl-1">
          <div
            v-for="child in childrenOf(category.id)"
            :key="child.id"
            class="flex h-8 items-center rounded-md"
            :class="[
              isActive(child.id) ? 'bg-active text-fg' : 'text-fg-3 hover:bg-hover hover:text-fg-2',
              dragId === child.id ? 'opacity-40' : '',
              dragOverId === child.id ? 'ring-1 ring-accent' : '',
            ]"
            draggable="true"
            @dragstart="onDragStart($event, child)"
            @dragover="onDragOver($event, child)"
            @drop="onDrop($event, child)"
            @dragend="clearDrag"
            @contextmenu.prevent="openMenu($event, child)"
            @dblclick="openEditor(child)"
          >
            <button type="button" class="flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px]" @click="choose(child)">
              <span :style="{ color: child.color }"><CategoryIcon :name="child.icon" :size="13" /></span>
              <span class="min-w-0 flex-1 truncate">{{ child.name }}</span>
              <span class="tabular text-[11px] text-fg-4">{{ child.package_count }}</span>
            </button>
          </div>
        </div>
      </template>

      <div v-if="roots.length === 0" class="px-2 py-1 text-[12px] text-fg-4">
        {{ t("sidebar.empty") }}
      </div>

      <!-- 其他入口 -->
      <div class="mt-3 border-t border-line pt-2">
        <button
          type="button"
          class="flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px] transition-colors"
          :class="route.name === 'tags' ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
          @click="router.push('/tags')"
        >
          <Tag :size="14" :stroke-width="1.75" class="shrink-0 text-fg-3" />
          <span class="min-w-0 flex-1 truncate">{{ t("sidebar.tags") }}</span>
        </button>
        <button
          type="button"
          class="flex h-8 w-full items-center gap-1.5 rounded-md px-2 text-left text-[12px] transition-colors"
          :class="route.name === 'storage' ? 'bg-active text-fg' : 'text-fg-2 hover:bg-hover hover:text-fg'"
          @click="router.push('/storage')"
        >
          <HardDrive :size="14" :stroke-width="1.75" class="shrink-0 text-fg-3" />
          <span class="min-w-0 flex-1 truncate">{{ t("sidebar.storage") }}</span>
        </button>
      </div>
    </nav>

    <ContextMenu
      v-if="menu"
      :items="menuItems"
      :x="menu.x"
      :y="menu.y"
      @select="onMenuSelect"
      @close="menu = null"
    />

    <CategoryEditor
      :open="Boolean(editor)"
      :category="editor?.category ?? null"
      :parent-id="editor?.parentId ?? null"
      @close="editor = null"
    />
  </aside>
</template>
