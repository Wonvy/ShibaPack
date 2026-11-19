// 侧栏：固定入口、分类树、标签与存储。

import { ChevronRight, HardDrive, LayoutGrid, type LucideIcon, MonitorCheck, Pencil, Plus, Tags, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { canMoveCategory, moveCategory } from "../lib/actions";
import { api } from "../lib/api";
import { beginDrag, useDrag } from "../lib/drag";
import { CATEGORY_COLORS, CATEGORY_ICONS, categoryIcon } from "../lib/icons";
import { childrenOf, flattenTree, newId, withDescendants } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { type Route, type Scope, useApp, useT } from "../lib/store";
import type { Category } from "../lib/types";
import { confirm } from "./dialogs";
import { Button, ContextMenu, Field, IconButton, Modal, Select, TextInput, cx } from "./ui";

type Editing = { mode: "new"; parentId: string | null } | { mode: "edit"; category: Category };

export function Sidebar() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const route = useApp((state) => state.route);
  const scope = useApp((state) => state.scope);
  const setScope = useApp((state) => state.setScope);
  const setRoute = useApp((state) => state.setRoute);
  const apply = useApp((state) => state.apply);
  const width = usePrefs((state) => state.sidebarWidth);
  const [editing, setEditing] = useState<Editing | null>(null);

  // 每个分类下的软件数，包含下级分类里的
  const counts = useMemo(() => {
    const direct = new Map<string, number>();
    for (const software of library.software) {
      if (software.categoryId) direct.set(software.categoryId, (direct.get(software.categoryId) ?? 0) + 1);
    }
    const total = new Map<string, number>();
    for (const category of library.categories) {
      let sum = 0;
      for (const id of withDescendants(library.categories, category.id)) sum += direct.get(id) ?? 0;
      total.set(category.id, sum);
    }
    return total;
  }, [library]);
  const uncategorized = library.software.filter((s) => !s.categoryId).length;
  // 正在拖一个软件时，“未分类”即使是空的也要露出来，好让人把软件拖回去
  const draggingSoftware = useDrag((state) => state.payload?.kind === "software");
  const overUncategorized = useDrag((state) => state.target !== null && state.target.category === null);

  const inLibrary = route === "library";
  const isScope = (target: Scope) =>
    inLibrary && scope.kind === target.kind && (scope.kind !== "category" || scope.id === (target as { id: string }).id);

  const remove = async (category: Category) => {
    const { ok } = await confirm({
      title: t("cat.delete"),
      body: t("cat.deleteConfirm", { name: category.name }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (ok) await apply(() => api.setCategories(library.categories.filter((c) => c.id !== category.id)));
  };

  return (
    <nav className="flex shrink-0 flex-col bg-side" style={{ width }}>
      <div className="flex flex-col gap-px p-1.5 pb-0">
        <NavRow icon={MonitorCheck} label={t("nav.installed")} active={route === "installed"} onClick={() => setRoute("installed")} />
        <NavRow
          icon={LayoutGrid}
          label={t("nav.all")}
          count={library.software.length}
          active={isScope({ kind: "all" })}
          onClick={() => setScope({ kind: "all" })}
        />
      </div>

      <div className="mt-2 flex h-7 shrink-0 items-center justify-between pr-1.5 pl-3.5">
        <span className="text-[11px] font-medium text-fg-3">{t("nav.categories")}</span>
        <IconButton label={t("cat.new")} className="size-6" onClick={() => setEditing({ mode: "new", parentId: null })}>
          <Plus size={14} strokeWidth={1.75} />
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
        <Tree
          parentId={null}
          depth={0}
          counts={counts}
          isActive={(id) => isScope({ kind: "category", id })}
          onSelect={(id) => setScope({ kind: "category", id })}
          onEdit={(category) => setEditing({ mode: "edit", category })}
          onAddChild={(id) => setEditing({ mode: "new", parentId: id })}
          onDelete={(category) => void remove(category)}
        />
        {library.categories.length === 0 && <div className="px-2 py-1.5 text-[12px] text-fg-4">{t("cat.empty")}</div>}
        {(uncategorized > 0 || draggingSoftware) && (
          <NavRow
            label={t("nav.uncategorized")}
            count={uncategorized}
            muted
            drop="uncategorized"
            dropping={overUncategorized}
            active={isScope({ kind: "uncategorized" })}
            onClick={() => setScope({ kind: "uncategorized" })}
          />
        )}
      </div>

      <div className="flex shrink-0 flex-col gap-px border-t border-line p-1.5">
        <RouteRow route="tags" icon={Tags} label={t("nav.tags")} count={library.tags.length} />
        <RouteRow route="storage" icon={HardDrive} label={t("nav.storage")} />
      </div>

      {editing && <CategoryDialog editing={editing} onClose={() => setEditing(null)} />}
    </nav>
  );
}

function RouteRow({ route, icon, label, count }: { route: Route; icon: LucideIcon; label: string; count?: number }) {
  const current = useApp((state) => state.route);
  const setRoute = useApp((state) => state.setRoute);
  return <NavRow icon={icon} label={label} count={count} active={current === route} onClick={() => setRoute(route)} />;
}

function NavRow({
  icon: Icon,
  label,
  count,
  active,
  muted,
  drop,
  dropping,
  onClick,
}: {
  icon?: LucideIcon;
  label: string;
  count?: number;
  active: boolean;
  muted?: boolean;
  /** 可以把东西拖到这一行上时的落点标识。 */
  drop?: string;
  dropping?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      data-drop={drop}
      onClick={onClick}
      className={cx(
        "flex h-8 w-full shrink-0 items-center gap-2 rounded-md px-2 text-left text-[13px] transition-colors",
        dropping
          ? "bg-accent-soft text-accent"
          : active
            ? "bg-active text-fg"
            : cx(muted ? "text-fg-3" : "text-fg-2", "hover:bg-hover hover:text-fg"),
      )}
    >
      {Icon ? <Icon size={16} strokeWidth={1.75} aria-hidden className="shrink-0" /> : <span className="w-4 shrink-0" />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && count > 0 && <span className="tabular text-[11px] text-fg-3">{count}</span>}
    </button>
  );
}

interface TreeProps {
  parentId: string | null;
  depth: number;
  counts: Map<string, number>;
  isActive: (id: string) => boolean;
  onSelect: (id: string) => void;
  onEdit: (category: Category) => void;
  onAddChild: (id: string) => void;
  onDelete: (category: Category) => void;
}

function Tree(props: TreeProps) {
  const t = useT();
  const categories = useApp((state) => state.library!.categories);
  const collapsed = usePrefs((state) => state.collapsed);
  const toggle = usePrefs((state) => state.toggleCollapsed);
  const dropTarget = useDrag((state) => state.target);
  const { parentId, depth, counts, isActive, onSelect, onEdit, onAddChild, onDelete } = props;

  return (
    <div className="flex flex-col gap-px">
      {childrenOf(categories, parentId).map((category) => {
        const hasChildren = categories.some((c) => c.parentId === category.id);
        const open = !collapsed.includes(category.id);
        const Icon = categoryIcon(category.icon);
        const count = counts.get(category.id) ?? 0;
        // 有东西正拖到这一行上：放进去时整行变色，排到前后时在上沿或下沿画一条线
        const zone = dropTarget?.category === category.id ? dropTarget.zone : null;
        return (
          <div key={category.id}>
            <ContextMenu
              items={[
                { label: t("cat.newChild"), icon: Plus, onSelect: () => onAddChild(category.id) },
                { label: t("cat.edit"), icon: Pencil, onSelect: () => onEdit(category) },
                { label: t("cat.delete"), icon: Trash2, danger: true, separated: true, onSelect: () => onDelete(category) },
              ]}
            >
              <div
                role="button"
                tabIndex={0}
                data-drop={`category:${category.id}`}
                onClick={() => onSelect(category.id)}
                onDoubleClick={() => onEdit(category)}
                onKeyDown={(event) => event.key === "Enter" && onSelect(category.id)}
                onPointerDown={(event) =>
                  beginDrag(
                    event,
                    { kind: "category", id: category.id, label: category.name },
                    (target) => void moveCategory(category.id, target),
                    (target) => canMoveCategory(categories, category.id, target),
                  )
                }
                style={{ paddingLeft: 4 + depth * 14 }}
                className={cx(
                  "flex h-8 items-center gap-1 rounded-md pr-2 text-[13px] transition-colors",
                  zone === "inside"
                    ? "bg-accent-soft text-accent"
                    : isActive(category.id)
                      ? "bg-active text-fg"
                      : "text-fg-2 hover:bg-hover hover:text-fg",
                  zone === "before" && "shadow-[inset_0_2px_0_var(--accent)]",
                  zone === "after" && "shadow-[inset_0_-2px_0_var(--accent)]",
                )}
              >
                <button
                  type="button"
                  aria-label={category.name}
                  tabIndex={-1}
                  onClick={(event) => {
                    event.stopPropagation();
                    toggle(category.id);
                  }}
                  className={cx("flex size-4 shrink-0 items-center justify-center rounded text-fg-3", !hasChildren && "invisible")}
                >
                  <ChevronRight size={12} strokeWidth={2} className={cx("transition-transform", open && "rotate-90")} />
                </button>
                {/* 图标默认单色；只有给分类选了颜色，图标才带那个颜色 */}
                <Icon size={16} strokeWidth={1.75} aria-hidden className="mr-1 shrink-0" style={{ color: category.color ?? undefined }} />
                <span className="min-w-0 flex-1 truncate">{category.name}</span>
                {count > 0 && <span className="tabular text-[11px] text-fg-3">{count}</span>}
              </div>
            </ContextMenu>
            {hasChildren && open && depth < 12 && <Tree {...props} parentId={category.id} depth={depth + 1} />}
          </div>
        );
      })}
    </div>
  );
}

function CategoryDialog({ editing, onClose }: { editing: Editing; onClose: () => void }) {
  const t = useT();
  const categories = useApp((state) => state.library!.categories);
  const apply = useApp((state) => state.apply);
  const original = editing.mode === "edit" ? editing.category : null;
  const [name, setName] = useState(original?.name ?? "");
  const [parentId, setParentId] = useState<string | null>(original?.parentId ?? (editing.mode === "new" ? editing.parentId : null));
  const [color, setColor] = useState<string | null>(original?.color ?? null);
  const [icon, setIcon] = useState<string | null>(original?.icon ?? null);

  // 不能把分类挪到它自己或它的下级分类下面
  const blocked = original ? withDescendants(categories, original.id) : new Set<string>();
  const parents = flattenTree(categories).filter((entry) => !blocked.has(entry.category.id));

  const save = async () => {
    if (!name.trim()) return;
    const next: Category = { id: original?.id ?? newId(), name: name.trim(), parentId, color, icon };
    const list = original ? categories.map((c) => (c.id === original.id ? next : c)) : [...categories, next];
    if (await apply(() => api.setCategories(list))) onClose();
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={original ? t("cat.edit") : editing.mode === "new" && editing.parentId ? t("cat.newChild") : t("cat.new")}
      width={380}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!name.trim()} onClick={() => void save()}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        <Field label={t("cat.name")}>
          <TextInput autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
        </Field>
        <Field label={t("cat.parent")}>
          <Select value={parentId ?? ""} onChange={(e) => setParentId(e.target.value || null)}>
            <option value="">{t("cat.top")}</option>
            {parents.map(({ category, depth }) => (
              <option key={category.id} value={category.id}>
                {"　".repeat(depth) + category.name}
              </option>
            ))}
          </Select>
        </Field>
        <div>
          <span className="mb-1.5 block text-[12px] font-medium text-fg-2">{t("cat.color")}</span>
          <div className="flex items-center gap-1.5">
            {CATEGORY_COLORS.map((value) => (
              <button
                key={value}
                type="button"
                aria-label={value}
                aria-pressed={color === value}
                onClick={() => setColor(color === value ? null : value)}
                style={{ background: value }}
                className={cx("size-5 rounded-full", color === value && "ring-2 ring-fg ring-offset-2 ring-offset-panel")}
              />
            ))}
            <input
              type="color"
              aria-label={t("cat.color")}
              value={color ?? "#5aa2ff"}
              onChange={(e) => setColor(e.target.value)}
              className="ml-1 size-6 cursor-pointer rounded border border-line-strong bg-transparent p-0"
            />
          </div>
        </div>
        <div>
          <span className="mb-1.5 block text-[12px] font-medium text-fg-2">{t("cat.icon")}</span>
          <div className="grid grid-cols-7 gap-1">
            {Object.entries(CATEGORY_ICONS).map(([key, Icon]) => (
              <button
                key={key}
                type="button"
                aria-label={key}
                aria-pressed={(icon ?? "folder") === key}
                onClick={() => setIcon(key)}
                className={cx(
                  "flex h-8 items-center justify-center rounded-md transition-colors",
                  (icon ?? "folder") === key ? "bg-accent-soft text-accent" : "text-fg-2 hover:bg-hover",
                )}
              >
                <Icon size={16} strokeWidth={1.75} style={{ color: (icon ?? "folder") === key ? undefined : (color ?? undefined) }} />
              </button>
            ))}
          </div>
        </div>
      </div>
    </Modal>
  );
}
