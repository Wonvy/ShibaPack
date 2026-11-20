// 侧栏：上面是软件库的几个入口，中间是分类树，下面是存储与设置。

import {
  ArrowDown,
  ArrowUp,
  ArrowUpCircle,
  ArrowUpToLine,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  FolderPlus,
  HardDrive,
  Inbox,
  LayoutGrid,
  type LucideIcon,
  MonitorCheck,
  Pencil,
  Plus,
  RefreshCw,
  Settings,
  Sparkles,
  Star,
  Trash2,
} from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { canMoveCategory, classify, moveCategory } from "../lib/actions";
import { api } from "../lib/api";
import { beginDrag, useDrag } from "../lib/drag";
import { CATEGORY_COLORS, CATEGORY_ICONS, categoryIcon } from "../lib/icons";
import { childrenOf, flattenTree, hasUpdate, newId, withDescendants } from "../lib/library";
import { usePrefs } from "../lib/prefs";
import { type Scope, useApp, useT } from "../lib/store";
import type { Category } from "../lib/types";
import { confirm } from "./dialogs";
import { Button, ContextMenu, Field, IconButton, type MenuEntry, Modal, Select, TextInput, cx } from "./ui";

type Editing = { mode: "new"; parentId: string | null } | { mode: "edit"; category: Category };

export function Sidebar() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const route = useApp((state) => state.route);
  const scope = useApp((state) => state.scope);
  const settingsOpen = useApp((state) => state.settingsOpen);
  const setScope = useApp((state) => state.setScope);
  const setRoute = useApp((state) => state.setRoute);
  const patch = useApp((state) => state.patch);
  const apply = useApp((state) => state.apply);
  const checkUpdates = useApp((state) => state.checkUpdates);
  const checking = useApp((state) => state.checking !== null);
  const installedCount = useApp((state) => state.installed?.length);
  const width = usePrefs((state) => state.sidebarWidth);
  const setPrefs = usePrefs((state) => state.set);
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
  const uncategorized = library.software.filter((s) => !s.categoryId);
  const starred = library.software.filter((s) => s.starred).length;
  const updates = library.software.filter(hasUpdate).length;
  // 正在拖一个软件时，“未分类”即使是空的也要露出来，好让人把软件拖回去
  const draggingSoftware = useDrag((state) => state.payload?.kind === "software");
  const overUncategorized = useDrag((state) => state.target !== null && state.target.category === null);

  const isScope = (target: Scope) =>
    route === "library" && scope.kind === target.kind && (scope.kind !== "category" || scope.id === (target as { id: string }).id);

  /** 右键菜单里的“智能分类…”：对这一组软件出建议。 */
  const classifyMenu = (ids: string[]): MenuEntry[] => [
    { label: t("classify.menu"), icon: Sparkles, disabled: ids.length === 0, onSelect: () => classify(ids) },
  ];
  const idsInCategory = (id: string) => {
    const ids = withDescendants(library.categories, id);
    return library.software.filter((s) => s.categoryId && ids.has(s.categoryId)).map((s) => s.id);
  };

  // 有下级的分类才能展开或折叠
  const parents = library.categories.filter((c) => library.categories.some((child) => child.parentId === c.id)).map((c) => c.id);
  /** “分类”标题和分类树空白处的右键菜单。 */
  const sectionMenu: MenuEntry[] = [
    { label: t("cat.new"), icon: Plus, onSelect: () => setEditing({ mode: "new", parentId: null }) },
    { label: t("cat.expandAll"), icon: ChevronsUpDown, separated: true, disabled: parents.length === 0, onSelect: () => setPrefs({ collapsed: [] }) },
    { label: t("cat.collapseAll"), icon: ChevronsDownUp, disabled: parents.length === 0, onSelect: () => setPrefs({ collapsed: parents }) },
  ];
  /** 一个分类的右键菜单。上移、下移在同级分类之间挪；移到顶层排在它原来的顶层上级后面。 */
  const categoryMenu = (category: Category): MenuEntry[] => {
    const peers = childrenOf(library.categories, category.parentId ?? null);
    const at = peers.findIndex((c) => c.id === category.id);
    let root = category;
    while (root.parentId) root = library.categories.find((c) => c.id === root.parentId) ?? { ...root, parentId: null };
    return [
      { label: t("cat.newChild"), icon: Plus, onSelect: () => setEditing({ mode: "new", parentId: category.id }) },
      { label: t("cat.newSibling"), icon: FolderPlus, onSelect: () => setEditing({ mode: "new", parentId: category.parentId ?? null }) },
      { label: t("cat.edit"), icon: Pencil, onSelect: () => setEditing({ mode: "edit", category }) },
      {
        label: t("cat.moveUp"),
        icon: ArrowUp,
        separated: true,
        disabled: at <= 0,
        onSelect: () => void moveCategory(category.id, { category: peers[at - 1].id, zone: "before" }),
      },
      {
        label: t("cat.moveDown"),
        icon: ArrowDown,
        disabled: at < 0 || at >= peers.length - 1,
        onSelect: () => void moveCategory(category.id, { category: peers[at + 1].id, zone: "after" }),
      },
      ...(category.parentId && root.id !== category.id
        ? [{ label: t("cat.moveTop"), icon: ArrowUpToLine, onSelect: () => void moveCategory(category.id, { category: root.id, zone: "after" }) }]
        : []),
      ...classifyMenu(idsInCategory(category.id)).map((entry) => ({ ...entry, separated: true })),
      { label: t("cat.delete"), icon: Trash2, danger: true, separated: true, onSelect: () => void remove(category) },
    ];
  };

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
      <div className="flex flex-col gap-px p-2 pb-0">
        <NavRow
          icon={LayoutGrid}
          label={t("nav.all")}
          count={library.software.length}
          active={isScope({ kind: "all" })}
          onClick={() => setScope({ kind: "all" })}
          menu={classifyMenu(library.software.map((s) => s.id))}
        />
        {/* 这台电脑上装的软件，常用，放在收藏上面 */}
        <NavRow
          icon={MonitorCheck}
          label={t("nav.installed")}
          count={installedCount}
          active={route === "installed"}
          onClick={() => setRoute("installed")}
        />
        <NavRow icon={Star} label={t("nav.starred")} count={starred} active={isScope({ kind: "starred" })} onClick={() => setScope({ kind: "starred" })} />
        <NavRow
          icon={ArrowUpCircle}
          label={t("nav.updates")}
          count={updates}
          highlight={updates > 0}
          active={isScope({ kind: "updates" })}
          onClick={() => setScope({ kind: "updates" })}
          action={
            // 检查在线更新：鼠标停在这一行上时出现，检查期间一直转着
            <span className={cx("flex", !checking && "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100")}>
              <IconButton
                label={t("scan.updates")}
                className="size-6"
                disabled={checking}
                onClick={(event) => {
                  event.stopPropagation();
                  void checkUpdates();
                }}
              >
                <RefreshCw size={13} strokeWidth={1.75} className={checking ? "animate-spin" : ""} />
              </IconButton>
            </span>
          }
        />
      </div>

      <ContextMenu items={sectionMenu}>
        <div className="mt-3 flex h-7 shrink-0 items-center justify-between pr-2 pl-4">
          <span className="text-[12px] font-medium text-fg-3">{t("nav.categories")}</span>
          <IconButton label={t("cat.new")} className="size-6" onClick={() => setEditing({ mode: "new", parentId: null })}>
            <Plus size={14} strokeWidth={1.75} />
          </IconButton>
        </div>
      </ContextMenu>

      {/* 右键分类以外的空白处：新建分类、全部展开或折叠。右键某个分类时出它自己的菜单 */}
      <ContextMenu items={sectionMenu}>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        <Tree
          parentId={null}
          depth={0}
          counts={counts}
          isActive={(id) => isScope({ kind: "category", id })}
          onSelect={(id) => setScope({ kind: "category", id })}
          menuFor={categoryMenu}
        />
        {library.categories.length === 0 && <div className="px-2 py-1.5 text-[12px] text-fg-4">{t("cat.empty")}</div>}
        {(uncategorized.length > 0 || draggingSoftware) && (
          <NavRow
            icon={Inbox}
            inTree
            label={t("nav.uncategorized")}
            count={uncategorized.length}
            muted
            drop="uncategorized"
            dropping={overUncategorized}
            active={isScope({ kind: "uncategorized" })}
            onClick={() => setScope({ kind: "uncategorized" })}
            menu={classifyMenu(uncategorized.map((s) => s.id))}
          />
        )}
      </div>
      </ContextMenu>

      {/* 下面是仓库存储和设置 */}
      <div className="flex shrink-0 flex-col gap-px border-t border-line p-2">
        <NavRow icon={HardDrive} label={t("nav.storage")} active={route === "storage"} onClick={() => setRoute("storage")} />
        <NavRow icon={Settings} label={t("app.settings")} active={settingsOpen} onClick={() => patch({ settingsOpen: true })} />
      </div>

      {editing && <CategoryDialog editing={editing} onClose={() => setEditing(null)} />}
    </nav>
  );
}

function NavRow({
  icon: Icon,
  label,
  count,
  active,
  muted,
  highlight,
  drop,
  dropping,
  menu,
  action,
  inTree,
  onClick,
}: {
  icon?: LucideIcon;
  label: string;
  count?: number;
  active: boolean;
  muted?: boolean;
  /** 数字用强调色（有更新时）。 */
  highlight?: boolean;
  /** 可以把东西拖到这一行上时的落点标识。 */
  drop?: string;
  dropping?: boolean;
  menu?: MenuEntry[];
  /** 数字左边的小按钮。 */
  action?: ReactNode;
  /** 放在分类树里：图标和文字跟顶层分类对齐。 */
  inTree?: boolean;
  onClick: () => void;
}) {
  const row = (
    <div
      role="button"
      tabIndex={0}
      data-drop={drop}
      onClick={onClick}
      onKeyDown={(event) => event.target === event.currentTarget && event.key === "Enter" && onClick()}
      className={cx(
        "group flex h-8 w-full shrink-0 items-center rounded-md text-left text-[13px] transition-colors",
        inTree ? "gap-1 pr-2 pl-1" : "gap-2 px-2",
        dropping
          ? "bg-accent-soft text-accent"
          : active
            ? "bg-active text-fg"
            : cx(muted ? "text-fg-3" : "text-fg-2", "hover:bg-hover hover:text-fg"),
      )}
    >
      {/* 分类树里每行前面有展开箭头的位置 */}
      {inTree && <span className="w-4 shrink-0" />}
      {Icon ? <Icon size={16} strokeWidth={1.75} aria-hidden className={cx("shrink-0", inTree && "mr-1")} /> : <span className="w-4 shrink-0" />}
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {action}
      {count !== undefined && count > 0 && (
        <span className={cx("tabular text-[12px]", highlight ? "font-medium text-accent" : "text-fg-3")}>{count}</span>
      )}
    </div>
  );
  return menu ? <ContextMenu items={menu}>{row}</ContextMenu> : row;
}

interface TreeProps {
  parentId: string | null;
  depth: number;
  counts: Map<string, number>;
  isActive: (id: string) => boolean;
  onSelect: (id: string) => void;
  menuFor: (category: Category) => MenuEntry[];
}

function Tree(props: TreeProps) {
  const categories = useApp((state) => state.library!.categories);
  const collapsed = usePrefs((state) => state.collapsed);
  const toggle = usePrefs((state) => state.toggleCollapsed);
  const dropTarget = useDrag((state) => state.target);
  const { parentId, depth, counts, isActive, onSelect, menuFor } = props;

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
            <ContextMenu items={menuFor(category)}>
              <div
                role="button"
                tabIndex={0}
                data-drop={`category:${category.id}`}
                onClick={() => onSelect(category.id)}
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
                {count > 0 && <span className="tabular text-[12px] text-fg-3">{count}</span>}
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
            {CATEGORY_COLORS.map((value, index) => (
              <button
                key={value}
                type="button"
                aria-label={t("cat.colorN", { n: index + 1 })}
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
