// 顶栏的搜索框。输入时照常过滤当前这一页的列表，同时在下面列出各处找到的结果：软件库、已安装、启动与服务，
// 还有 GitHub、Homebrew 上的软件。右端选搜索范围。
//
// - 本机的结果马上出来；GitHub、Homebrew 等输入停下来一会儿才去搜（GitHub 不登录每分钟只能搜 10 次），搜过的词记着不再搜。
// - 点本机的结果跳过去选中它；点在线的结果打开“从在线源添加”，查出最新版本，用户确认后才添加。
// - 范围是 GitHub 或 Homebrew 时，输入的字不拿来过滤当前页的列表。
// - Ctrl / ⌘ + F 跳到这里；↑↓ 选结果，Enter 打开；Esc 先关掉结果，再按一次清空。

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ExternalLink, Layers, LayoutGrid, LoaderCircle, MonitorCheck, Rocket, Search, X } from "lucide-react";
import { type PointerEvent as ReactPointerEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { siGithub, siHomebrew } from "simple-icons";
import { api } from "../lib/api";
import { type Key, errorText } from "../lib/i18n";
import { primaryVersion } from "../lib/library";
import { SEARCH_SCOPES, type SearchScope, usePrefs } from "../lib/prefs";
import { useStartup } from "../lib/startup";
import { useApp, useLanguage, useT } from "../lib/store";
import type { Candidate, RepoHit } from "../lib/types";
import { InstalledSoftwareIcon, RepositorySoftwareIcon } from "./SoftwareIcon";
import { StartupIcon } from "./StartupIcon";
import { ContextMenu, cx, itemClass, menuClass } from "./ui";

type Group = "library" | "installed" | "startup" | "github" | "homebrew";

interface Result {
  key: string;
  title: string;
  subtitle: string;
  /** 右边的小字：版本号、星数。 */
  meta?: string;
  /** 名字后面的小标签：已在软件库、已归档。 */
  badge?: string;
  icon: ReactNode;
  /** 鼠标停上去时的提示。 */
  hint?: string;
  run: () => void;
  /** 在浏览器里打开的网页（GitHub 仓库）。 */
  link?: string;
}

interface Section {
  group: Group;
  items: Result[];
  loading: boolean;
  /** 搜不了时的原因，或者第一次下载 Homebrew 目录的说明。 */
  note: string | null;
}

const SCOPE_ICONS: Record<SearchScope, ReactNode> = {
  all: <Layers size={14} strokeWidth={1.75} />,
  library: <LayoutGrid size={14} strokeWidth={1.75} />,
  installed: <MonitorCheck size={14} strokeWidth={1.75} />,
  startup: <Rocket size={14} strokeWidth={1.75} />,
  github: <Brand path={siGithub.path} size={13} />,
  homebrew: <Brand path={siHomebrew.path} size={13} />,
};

const ONLINE: SearchScope[] = ["github", "homebrew"];
/** 每一组最多列几个。只看这一处时多列一些。 */
const FEW = 5;
const MANY = 12;

function Brand({ path, size }: { path: string; size: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden className="shrink-0">
      <path d={path} />
    </svg>
  );
}

/** 在线结果左边的方块图标。 */
function BrandTile({ path }: { path: string }) {
  return (
    <span aria-hidden className="flex size-6 shrink-0 items-center justify-center rounded-md bg-tile text-tile-fg">
      <Brand path={path} size={13} />
    </span>
  );
}

function GitHubOwnerAvatar({ url }: { url?: string }) {
  const [failedUrl, setFailedUrl] = useState<string>();
  if (!url || failedUrl === url) return <BrandTile path={siGithub.path} />;
  return (
    <img
      src={url}
      alt=""
      draggable={false}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setFailedUrl(url)}
      className="size-6 shrink-0 rounded-md bg-tile object-cover"
    />
  );
}

/** 12345 → 12.3k */
function compact(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1).replace(/\.0$/, "")}k` : String(n);
}

/** 名字以搜索词开头的排在前面，其次是包含的；都不包含的不要。 */
function rank(query: string, ...texts: string[]): number {
  const q = query.toLowerCase();
  let best = Infinity;
  texts.forEach((text, field) => {
    const at = text.toLowerCase().indexOf(q);
    if (at >= 0) best = Math.min(best, field * 10 + (at === 0 ? 0 : 1));
  });
  return best;
}

function pick<T>(list: T[], query: string, texts: (item: T) => string[], limit: number): T[] {
  return list
    .map((item) => ({ item, score: rank(query, ...texts(item)) }))
    .filter((entry) => entry.score !== Infinity)
    .sort((a, b) => a.score - b.score)
    .slice(0, limit)
    .map((entry) => entry.item);
}

// 搜过的词记着，这次运行里不再搜第二遍（GitHub 每分钟只能搜 10 次）
const githubCache = new Map<string, RepoHit[]>();
const homebrewCache = new Map<string, Candidate[]>();

/** 在线搜索：输入停下来 `delay` 毫秒后才搜，至少两个字。新结果回来之前先留着旧的，免得结果区一闪一闪。 */
function useOnline<T>(enabled: boolean, query: string, cache: Map<string, T[]>, search: (q: string) => Promise<T[]>, delay: number) {
  const language = useLanguage();
  const [state, setState] = useState<{ hits: T[] | null; loading: boolean; error: string | null }>({ hits: null, loading: false, error: null });
  useEffect(() => {
    const q = query.trim();
    if (!enabled || q.length < 2) {
      setState({ hits: null, loading: false, error: null });
      return;
    }
    const key = q.toLowerCase();
    const cached = cache.get(key);
    if (cached) {
      setState({ hits: cached, loading: false, error: null });
      return;
    }
    let stale = false;
    setState((previous) => ({ ...previous, loading: true, error: null }));
    const timer = setTimeout(() => {
      search(q).then(
        (hits) => {
          cache.set(key, hits);
          if (!stale) setState({ hits, loading: false, error: null });
        },
        (error) => {
          if (!stale) setState({ hits: null, loading: false, error: errorText(language, error) });
        },
      );
    }, delay);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [enabled, query, cache, search, delay, language]);
  return state;
}

export function SearchBox() {
  const t = useT();
  const search = useApp((state) => state.search);
  const patch = useApp((state) => state.patch);
  const scope = usePrefs((state) => state.searchScope);
  const shortcuts = usePrefs((state) => state.searchShortcuts);
  const setPrefs = usePrefs((state) => state.set);
  const online = ONLINE.includes(scope);
  // 范围是 GitHub、Homebrew 时，字只用来在网上搜，不过滤当前页的列表
  const [onlineText, setOnlineText] = useState("");
  const text = online ? onlineText : search;
  const setText = (value: string) => (online ? setOnlineText(value) : patch({ search: value }));
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(-1);
  const box = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);

  const sections = useResults(text, scope, open);
  const flat = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const busy = sections.some((section) => section.loading);
  const shown = open && text.trim() !== "";

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        input.current?.focus();
        input.current?.select();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // 点到搜索框和结果以外的地方就收起结果
  useEffect(() => {
    if (!open) return;
    const onDown = (event: PointerEvent) => {
      if (!box.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onDown);
    return () => document.removeEventListener("pointerdown", onDown);
  }, [open]);

  // 字变了、范围变了，键盘选中的那一项回到没有
  useEffect(() => setActive(-1), [text, scope]);

  useEffect(() => {
    if (active >= 0) box.current?.querySelector(`[data-result="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const choose = (result: Result) => {
    setOpen(false);
    input.current?.blur();
    result.run();
  };

  const changeScope = (next: SearchScope) => {
    const nextOnline = ONLINE.includes(next);
    // 已经输入的字跟着带过去；从网上的范围换回来时，才拿它过滤当前页
    if (nextOnline && !online) {
      setOnlineText(search);
      patch({ search: "" });
    } else if (!nextOnline && online) {
      patch({ search: onlineText });
      setOnlineText("");
    }
    if (next !== scope) setPrefs({ searchScope: next });
    setOpen(true);
    requestAnimationFrame(() => input.current?.focus());
  };

  const placeholder =
    scope === "all"
      ? t("search.placeholder.all")
      : scope === "library"
        ? t("lib.search")
        : scope === "installed"
          ? t("installed.search")
          : scope === "startup"
            ? t("startup.search")
            : t(`search.placeholder.${scope}` as Key);
  return (
    <div ref={box} className="@container relative w-full max-w-[400px] min-w-[180px]">
      <ContextMenu items={SEARCH_SCOPES.map((value) => ({
        label: t(`search.scope.${value}`),
        checked: shortcuts.includes(value),
        onSelect: () => {
          const { searchShortcuts, set } = usePrefs.getState();
          set({ searchShortcuts: searchShortcuts.includes(value) ? searchShortcuts.filter((entry) => entry !== value) : [...searchShortcuts, value] });
        },
      }))}>
        <div className="flex h-7 items-center gap-1.5 rounded-md border border-transparent bg-hover pr-1 focus-within:border-accent">
          <label className="flex h-full min-w-0 flex-1 items-center gap-1.5 pl-2.5">
            <Search size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
            <input
              ref={input}
              type="text"
              value={text}
              spellCheck={false}
              role="combobox"
              aria-label={placeholder}
              aria-expanded={shown}
              aria-controls="search-results"
              placeholder={placeholder}
              onChange={(event) => {
                setText(event.target.value);
                setOpen(true);
              }}
              onFocus={() => setOpen(true)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  event.preventDefault();
                  // 先收起结果；结果已经收起了再清空
                  if (shown) setOpen(false);
                  else {
                    setText("");
                    event.currentTarget.blur();
                  }
                } else if (event.key === "ArrowDown" && shown && flat.length > 0) {
                  event.preventDefault();
                  setActive((index) => Math.min(flat.length - 1, index + 1));
                } else if (event.key === "ArrowUp" && shown && flat.length > 0) {
                  event.preventDefault();
                  setActive((index) => Math.max(0, index - 1));
                } else if (event.key === "Enter" && shown && active >= 0 && flat[active]) {
                  event.preventDefault();
                  choose(flat[active]);
                }
              }}
              className="h-full min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3"
            />
          </label>
          <div className="flex shrink-0 items-center gap-0.5">
            {text && (
              <button
                type="button"
                aria-label={t("common.close")}
                onClick={() => {
                  setText("");
                  input.current?.focus();
                }}
                className="flex size-5 items-center justify-center rounded text-fg-3 hover:text-fg"
              >
                <X size={13} strokeWidth={2} />
              </button>
            )}
            {shortcuts.length > 0 && <SearchShortcuts order={shortcuts} scope={scope} onChange={changeScope} />}
            <SearchScopePicker scope={scope} onChange={changeScope} />
          </div>
        </div>
      </ContextMenu>

      {shown && (
        <div
          id="search-results"
          role="listbox"
          // 按着结果时不要让输入框失去焦点，键盘还能接着用
          onPointerDown={(event) => event.preventDefault()}
          className="absolute top-full left-1/2 z-50 mt-1.5 flex max-h-[min(520px,70vh)] w-[max(100%,440px)] -translate-x-1/2 animate-pop flex-col overflow-hidden rounded-lg bg-panel shadow-pop"
        >
          <div className="min-h-0 flex-1 overflow-y-auto p-1">
            {sections.map((section) => (
              <ResultSection key={section.group} section={section} offset={flat.indexOf(section.items[0])} active={active} onHover={setActive} onChoose={choose} />
            ))}
            {flat.length === 0 && !busy && sections.every((section) => !section.note) && (
              <div className="px-2 py-4 text-center text-[12px] text-fg-3">
                {online && text.trim().length < 2 ? t("search.typeMore") : t("search.noResults")}
              </div>
            )}
          </div>
          <div className="flex h-7 shrink-0 items-center border-t border-line px-3 text-[12px] text-fg-4">{t("search.keys")}</div>
        </div>
      )}
    </div>
  );
}

const SHORTCUT_HOLD_MS = 225;

interface ShortcutPreview {
  order: SearchScope[];
  value: SearchScope;
  offset: number;
}

function SearchShortcuts({ order, scope, onChange }: { order: SearchScope[]; scope: SearchScope; onChange: (next: SearchScope) => void }) {
  const t = useT();
  const strip = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: number; timer: number; preview: ShortcutPreview | null; from: number; startX: number; centers: number[] } | null>(null);
  const [preview, setPreview] = useState<ShortcutPreview | null>(null);
  const suppressClick = useRef(false);

  useEffect(() => () => window.clearTimeout(drag.current?.timer), []);

  const start = (event: ReactPointerEvent<HTMLButtonElement>, value: SearchScope) => {
    if (event.button !== 0 || event.ctrlKey || drag.current || !strip.current) return;
    // 触屏在快捷图标上长按用于拖拽，避免父级同时弹出右键菜单。
    if (event.pointerType !== "mouse") event.stopPropagation();
    suppressClick.current = false;
    const element = strip.current;
    const centers = Array.from(element.children).flatMap((button) => {
      const rect = button.getBoundingClientRect();
      return rect.width > 0 ? [rect.left + rect.width / 2] : [];
    });
    if (centers.length < 2) return;
    const id = event.pointerId;
    const timer = window.setTimeout(() => {
      const active = drag.current;
      if (!active || active.id !== id) return;
      // 捕获指针的容器在图标换位时保持不动。
      element.setPointerCapture(id);
      suppressClick.current = true;
      active.preview = { order, value, offset: 0 };
      setPreview(active.preview);
    }, SHORTCUT_HOLD_MS);
    drag.current = { id, timer, preview: null, from: order.indexOf(value), startX: event.clientX, centers };
  };

  const move = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = drag.current;
    if (!active?.preview || active.id !== event.pointerId) return;
    const { centers, from, startX } = active;
    const value = active.preview.value;
    const x = Math.min(centers[centers.length - 1], Math.max(centers[0], centers[from] + event.clientX - startX));
    const to = centers.reduce((closest, center, index) => Math.abs(center - x) < Math.abs(centers[closest] - x) ? index : closest, 0);
    const next = order.filter((entry) => entry !== value);
    next.splice(to, 0, value);
    active.preview = { order: next, value, offset: x - centers[to] };
    setPreview(active.preview);
  };

  const finish = (event: ReactPointerEvent<HTMLDivElement>, save = false) => {
    const active = drag.current;
    if (!active || active.id !== event.pointerId) return;
    window.clearTimeout(active.timer);
    drag.current = null;
    const next = active.preview?.order;
    if (save && next && next.some((value, index) => value !== order[index])) {
      usePrefs.getState().set({ searchShortcuts: next });
    }
    setPreview(null);
  };

  return (
    <div
      ref={strip}
      className={cx("flex items-center gap-0.5 border-r border-line pr-1", preview && "cursor-grabbing")}
      onPointerMove={move}
      onPointerUp={(event) => finish(event, true)}
      onPointerCancel={(event) => finish(event)}
      onPointerLeave={(event) => { if (!drag.current?.preview) finish(event); }}
      onLostPointerCapture={(event) => { if (event.target === event.currentTarget) finish(event); }}
    >
      {(preview?.order ?? order).map((value, index) => {
        const label = t(`search.scope.${value}`);
        const dragged = preview?.value === value;
        return (
          <button
            key={value}
            type="button"
            aria-label={label}
            aria-pressed={scope === value}
            title={label}
            onPointerDown={(event) => start(event, value)}
            onClick={(event) => { if (event.detail === 0 || !suppressClick.current) onChange(value); }}
            style={dragged ? { translate: `${preview.offset}px` } : undefined}
            className={cx(
              "size-5 touch-none items-center justify-center rounded text-fg-3",
              index < 3 ? "flex" : "hidden @[250px]:flex",
              dragged && "relative z-10 cursor-grabbing bg-active shadow-pop",
            )}
          >
            {SCOPE_ICONS[value]}
          </button>
        );
      })}
    </div>
  );
}

function SearchScopePicker({ scope, onChange }: { scope: SearchScope; onChange: (next: SearchScope) => void }) {
  const t = useT();
  const currentLabel = t(`search.scope.${scope}`);
  const item = cx(itemClass, "pr-2 text-fg");

  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>
        <button
          type="button"
          title={`${t("search.scope")} · ${currentLabel}`}
          aria-label={`${t("search.scope")} · ${currentLabel}`}
          className="flex h-5 items-center gap-1 rounded px-1.5 text-[12px] text-fg-3 transition-colors hover:bg-active hover:text-fg-2 data-[state=open]:bg-active"
        >
          <span className="flex shrink-0">{SCOPE_ICONS[scope]}</span>
          <span className="hidden max-w-20 truncate @[350px]:inline">{currentLabel}</span>
          <ChevronDown size={11} strokeWidth={2} aria-hidden className="shrink-0 opacity-60" />
        </button>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align="end" sideOffset={6} className={menuClass}>
          <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-[12px] text-fg-3">{t("search.scope")}</DropdownPrimitive.Label>
          <DropdownPrimitive.RadioGroup value={scope} onValueChange={(next) => onChange(next as SearchScope)}>
            {SEARCH_SCOPES.map((value) => (
              <div key={value}>
                {value === "github" && <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />}
                <DropdownPrimitive.RadioItem value={value} className={item}>
                  <span className="flex size-4 items-center justify-center text-fg-2">{SCOPE_ICONS[value]}</span>
                  <span className="flex-1">{t(`search.scope.${value}`)}</span>
                  <DropdownPrimitive.ItemIndicator>
                    <Check size={14} strokeWidth={2} className="text-accent" aria-hidden />
                  </DropdownPrimitive.ItemIndicator>
                </DropdownPrimitive.RadioItem>
              </div>
            ))}
          </DropdownPrimitive.RadioGroup>
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

function ResultSection({
  section,
  offset,
  active,
  onHover,
  onChoose,
}: {
  section: Section;
  offset: number;
  active: number;
  onHover: (index: number) => void;
  onChoose: (result: Result) => void;
}) {
  const t = useT();
  const attempt = useApp((state) => state.attempt);
  if (section.items.length === 0 && !section.loading && !section.note) return null;
  return (
    <div className="pb-1">
      <div className="flex h-7 items-center gap-1.5 px-2 text-[12px] font-medium text-fg-3">
        <span>{t(`search.scope.${section.group}` as Key)}</span>
        {section.loading && <LoaderCircle size={12} strokeWidth={2} className="animate-spin" aria-hidden />}
      </div>
      {section.note && <div className="px-2 pb-1.5 text-[12px] leading-relaxed text-fg-3">{section.note}</div>}
      {section.items.map((result, index) => {
        const at = offset + index;
        return (
          <div
            key={result.key}
            role="option"
            aria-selected={at === active}
            data-result={at}
            title={result.hint}
            onPointerMove={() => at !== active && onHover(at)}
            onClick={() => onChoose(result)}
            className={cx("group flex h-11 cursor-default items-center gap-2.5 rounded-md px-2", at === active && "bg-active")}
          >
            {result.icon}
            <div className="min-w-0 flex-1">
              <div className="flex min-w-0 items-center gap-1.5">
                <span className="truncate text-[13px]">{result.title}</span>
                {result.badge && <span className="shrink-0 rounded-[4px] bg-hover px-1 text-[12px] text-fg-3">{result.badge}</span>}
              </div>
              {result.subtitle && <div className="truncate text-[12px] text-fg-3">{result.subtitle}</div>}
            </div>
            {result.meta && <span className="tabular shrink-0 text-[12px] text-fg-3">{result.meta}</span>}
            {result.link && (
              <button
                type="button"
                title={t("search.openPage")}
                aria-label={t("search.openPage")}
                onClick={(event) => {
                  event.stopPropagation();
                  void attempt(() => api.openUrl(result.link!));
                }}
                className={cx(
                  "flex size-6 shrink-0 items-center justify-center rounded text-fg-3 hover:bg-hover hover:text-fg",
                  at === active ? "visible" : "invisible group-hover:visible",
                )}
              >
                <ExternalLink size={13} strokeWidth={1.75} />
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** 按范围在各处找，返回分好组的结果。`open` 为假时不去网上搜。 */
function useResults(text: string, scope: SearchScope, open: boolean): Section[] {
  const t = useT();
  const query = text.trim();
  const library = useApp((state) => state.library);
  const installed = useApp((state) => state.installed);
  const reveal = useApp((state) => state.reveal);
  const setRoute = useApp((state) => state.setRoute);
  const patch = useApp((state) => state.patch);
  const startup = useStartup((state) => state.items);
  const setPrefs = usePrefs((state) => state.set);
  const wants = (group: Group) => scope === "all" || scope === group;
  const limit = scope === "all" ? FEW : MANY;

  // 只看启动与服务、又还没列过时，现在列（“全部”里不主动列：macOS 上第一次列登录项会弹出授权提示）
  useEffect(() => {
    if (open && scope === "startup" && useStartup.getState().items === null) void useStartup.getState().load();
  }, [open, scope]);

  const github = useOnline(open && wants("github"), query, githubCache, api.githubSearch, 600);
  const homebrew = useOnline(open && wants("homebrew"), query, homebrewCache, api.sourceSearch, 300);

  return useMemo(() => {
    if (!query) return [];
    const sections: Section[] = [];
    const inLibrary = (kind: "github" | "homebrew", reference: string) =>
      library?.software.find(
        (software) => software.subscription?.source === kind && software.subscription.reference.toLowerCase() === reference.toLowerCase(),
      );

    if (wants("library") && library) {
      const found = pick(library.software, query, (s) => [s.name, s.publisher], limit);
      sections.push({
        group: "library",
        loading: false,
        note: null,
        items: found.map((software) => ({
          key: `library:${software.id}`,
          title: software.name,
          subtitle: software.publisher,
          meta: primaryVersion(software)?.version,
          icon: <RepositorySoftwareIcon software={software} size={24} />,
          run: () => reveal(software.id),
        })),
      });
    }
    if (wants("installed") && installed) {
      const found = pick(installed, query, (app) => [app.name, app.publisher], limit);
      sections.push({
        group: "installed",
        loading: false,
        note: null,
        items: found.map((app) => ({
          key: `installed:${app.id}`,
          title: app.name,
          subtitle: app.publisher,
          meta: app.version,
          icon: <InstalledSoftwareIcon app={app} size={24} />,
          run: () => {
            setRoute("installed");
            useApp.getState().selectInstalled(app.id);
          },
        })),
      });
    }
    if (wants("startup") && startup) {
      // “全部”里只列第三方的：系统自带的有好几百个，名字又都差不多
      const pool = scope === "startup" ? startup : startup.filter((item) => !item.system);
      const found = pick(pool, query, (item) => [item.name, item.app, item.label, item.description], limit);
      sections.push({
        group: "startup",
        loading: false,
        note: null,
        items: found.map((item) => ({
          key: `startup:${item.id}`,
          title: item.name,
          subtitle: item.description || item.app || item.command,
          meta: item.kind === "service" ? t("startup.tab.service") : t("startup.tab.startup"),
          icon: <StartupIcon item={item} size={24} />,
          run: () => {
            setRoute("startup");
            setPrefs({ startupTab: item.kind });
            useStartup.getState().select(item.id);
          },
        })),
      });
    }
    if (wants("github") && query.length >= 2) {
      sections.push({
        group: "github",
        loading: github.loading,
        note: github.error,
        items: (github.hits ?? []).slice(0, limit === MANY ? MANY : 6).map((hit) => {
          const existing = inLibrary("github", hit.reference);
          return {
            key: `github:${hit.reference}`,
            title: hit.reference,
            subtitle: hit.description,
            meta: `★ ${compact(hit.stars)}`,
            badge: existing ? t("search.inLibrary") : hit.archived ? t("search.archived") : undefined,
            hint: existing ? undefined : t("search.add"),
            icon: <GitHubOwnerAvatar url={hit.ownerAvatarUrl} />,
            link: hit.url,
            run: () =>
              existing ? reveal(existing.id) : patch({ sourceDialog: { softwareId: null, kind: "github", reference: hit.reference, name: hit.name } }),
          };
        }),
      });
    }
    if (wants("homebrew") && query.length >= 2) {
      const first = homebrew.loading && homebrew.hits === null && homebrewCache.size === 0;
      sections.push({
        group: "homebrew",
        loading: homebrew.loading,
        note: homebrew.error ?? (first ? t("source.firstSearch") : null),
        items: (homebrew.hits ?? []).slice(0, limit).map((hit) => {
          const existing = inLibrary("homebrew", hit.reference);
          return {
            key: `homebrew:${hit.reference}`,
            title: hit.name,
            subtitle: hit.description || hit.reference,
            meta: hit.version,
            badge: existing ? t("search.inLibrary") : undefined,
            hint: existing ? undefined : t("search.add"),
            icon: <BrandTile path={siHomebrew.path} />,
            run: () =>
              existing ? reveal(existing.id) : patch({ sourceDialog: { softwareId: null, kind: "homebrew", reference: hit.reference, name: hit.name } }),
          };
        }),
      });
    }
    return sections;
    // wants 和 limit 都由 scope 决定
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query, scope, library, installed, startup, github, homebrew, t, reveal, setRoute, patch, setPrefs]);
}
