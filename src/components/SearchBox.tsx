// 顶栏的搜索框。输入时照常过滤当前这一页的列表，同时在下面列出各处找到的结果：软件库、已安装、启动与服务，
// 还有 GitHub、Homebrew 上的软件。右端选搜索范围。
//
// - 本机的结果马上出来；GitHub、Homebrew 等输入停下来一会儿才去搜（GitHub 不登录每分钟只能搜 10 次），搜过的词记着不再搜。
// - 点本机的结果跳过去选中它；点在线的结果打开“从在线源添加”，查出最新版本，用户确认后才添加。
// - 范围是 GitHub 或 Homebrew 时，输入的字不拿来过滤当前页的列表。
// - Ctrl / ⌘ + F 跳到这里；↑↓ 选结果，Enter 打开；Esc 先关掉结果，再按一次清空。

import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, ExternalLink, Layers, LayoutGrid, LoaderCircle, MonitorCheck, Rocket, Search, X } from "lucide-react";
import { type ReactNode, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { siGithub, siHomebrew } from "simple-icons";
import { api } from "../lib/api";
import { type Key, errorText } from "../lib/i18n";
import { primaryVersion } from "../lib/library";
import { type SearchScope, usePrefs } from "../lib/prefs";
import { useStartup } from "../lib/startup";
import { useApp, useLanguage, useT } from "../lib/store";
import type { Candidate, RepoHit } from "../lib/types";
import { SoftwareIcon } from "./SoftwareIcon";
import { StartupIcon } from "./StartupIcon";
import { cx, itemClass, menuClass } from "./ui";

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

const SCOPES: { value: SearchScope; icon: ReactNode }[] = [
  { value: "all", icon: <Layers size={14} strokeWidth={1.75} /> },
  { value: "library", icon: <LayoutGrid size={14} strokeWidth={1.75} /> },
  { value: "installed", icon: <MonitorCheck size={14} strokeWidth={1.75} /> },
  { value: "startup", icon: <Rocket size={14} strokeWidth={1.75} /> },
  { value: "github", icon: <Brand path={siGithub.path} size={13} /> },
  { value: "homebrew", icon: <Brand path={siHomebrew.path} size={13} /> },
];

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
  const right = useRef<HTMLDivElement>(null);
  const [padRight, setPadRight] = useState(80);

  const sections = useResults(text, scope, open);
  const flat = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const busy = sections.some((section) => section.loading);
  const shown = open && text.trim() !== "";

  // 右端的清空按钮和范围按钮宽度会变，输入的字不要钻到它们下面
  useLayoutEffect(() => {
    if (right.current) setPadRight(right.current.offsetWidth + 10);
  }, [scope, text, t]);

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
    if (next === scope) return;
    const nextOnline = ONLINE.includes(next);
    // 已经输入的字跟着带过去；从网上的范围换回来时，才拿它过滤当前页
    if (nextOnline && !online) {
      setOnlineText(search);
      patch({ search: "" });
    } else if (!nextOnline && online) {
      patch({ search: onlineText });
      setOnlineText("");
    }
    setPrefs({ searchScope: next });
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
  const current = SCOPES.find((entry) => entry.value === scope) ?? SCOPES[0];

  return (
    <div ref={box} className="relative w-full max-w-[400px] min-w-[180px]">
      <Search size={14} strokeWidth={1.75} aria-hidden className="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-fg-3" />
      <input
        ref={input}
        type="text"
        value={text}
        spellCheck={false}
        role="combobox"
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
        style={{ paddingRight: padRight }}
        className="h-7 w-full rounded-md border border-transparent bg-hover pl-8 text-[13px] outline-none placeholder:text-fg-3 focus:border-accent"
      />
      <div ref={right} className="absolute inset-y-0 right-1 flex items-center gap-0.5">
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
        <DropdownPrimitive.Root>
          <DropdownPrimitive.Trigger asChild>
            <button
              type="button"
              title={t("search.scope")}
              aria-label={`${t("search.scope")} · ${t(`search.scope.${scope}` as Key)}`}
              className={cx(
                "flex h-5 items-center gap-1 rounded px-1.5 text-[12px] whitespace-nowrap text-fg-3 transition-colors outline-none hover:bg-active hover:text-fg-2",
                "data-[state=open]:bg-active",
              )}
            >
              <span className="flex">{current.icon}</span>
              <span>{t(`search.scope.${scope}` as Key)}</span>
              <ChevronDown size={11} strokeWidth={2} aria-hidden className="opacity-60" />
            </button>
          </DropdownPrimitive.Trigger>
          <DropdownPrimitive.Portal>
            <DropdownPrimitive.Content align="end" sideOffset={6} className={menuClass}>
              <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-[12px] text-fg-3">{t("search.scope")}</DropdownPrimitive.Label>
              <DropdownPrimitive.RadioGroup value={scope} onValueChange={(next) => changeScope(next as SearchScope)}>
                {SCOPES.map((entry, index) => (
                  <div key={entry.value}>
                    {/* 本机的几处和网上的分开 */}
                    {index === 4 && <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />}
                    <DropdownPrimitive.RadioItem value={entry.value} className={cx(itemClass, "pr-2 text-fg")}>
                      <span className="flex size-4 items-center justify-center text-fg-2">{entry.icon}</span>
                      <span className="flex-1">{t(`search.scope.${entry.value}` as Key)}</span>
                      <DropdownPrimitive.ItemIndicator>
                        <Check size={14} strokeWidth={2} className="text-accent" />
                      </DropdownPrimitive.ItemIndicator>
                    </DropdownPrimitive.RadioItem>
                  </div>
                ))}
              </DropdownPrimitive.RadioGroup>
            </DropdownPrimitive.Content>
          </DropdownPrimitive.Portal>
        </DropdownPrimitive.Root>
      </div>

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
  const root = useApp((state) => state.status?.repoRoot);
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
          icon: <SoftwareIcon name={software.name} icon={software.icon && root ? `${root}/${software.icon}` : null} size={24} />,
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
          icon: <SoftwareIcon name={app.name} icon={app.icon} size={24} />,
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
            icon: <BrandTile path={siGithub.path} />,
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
  }, [query, scope, library, root, installed, startup, github, homebrew, t, reveal, setRoute, patch, setPrefs]);
}
