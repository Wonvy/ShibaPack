import { type MouseEvent, type ReactNode, useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { fullPath, primaryVersion, shownSummary, sortedVersions } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { PackageFile, Software } from "../lib/types";

const OPEN_DELAY = 450;
const CLOSE_DELAY = 150;
const WIDTH = 420;
const GAP = 6;
const MARGIN = 8;

/** 悬停后展开，移进卡片可滚动查看路径，离开后收起。 */
export function useHoverCard(content: ReactNode, variant: "card" | "label" = "card") {
  const compact = variant === "label";
  const id = useId();
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const panel = useRef<HTMLDivElement>(null);
  const stay = useCallback(() => {
    window.clearTimeout(timer.current);
    timer.current = undefined;
  }, []);
  const hide = useCallback(() => {
    stay();
    setAnchor(null);
  }, [stay]);
  const leave = () => {
    stay();
    timer.current = window.setTimeout(hide, CLOSE_DELAY);
  };

  useEffect(() => () => window.clearTimeout(timer.current), []);
  useEffect(() => {
    if (!anchor) return;
    const outside = (event: Event) => {
      if (!(event.target instanceof Node && panel.current?.contains(event.target))) hide();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("scroll", outside, true);
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("resize", hide);
    window.addEventListener("blur", hide);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("scroll", outside, true);
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("resize", hide);
      window.removeEventListener("blur", hide);
      window.removeEventListener("keydown", escape);
    };
  }, [anchor, hide]);

  const handlers = {
    "aria-describedby": anchor ? id : undefined,
    "data-hover-label": compact ? "" : undefined,
    onMouseOver: (event: MouseEvent<HTMLElement>) => {
      const element = event.currentTarget;
      const label = event.target instanceof Element ? event.target.closest("[data-hover-label]") : null;
      if (label && label !== element) {
        hide();
        return;
      }
      if (anchor) {
        stay();
        return;
      }
      if (timer.current !== undefined) return;
      timer.current = window.setTimeout(() => {
        timer.current = undefined;
        const menuOpen = element.matches('[data-state="open"]') || element.querySelector('[aria-haspopup="menu"][data-state="open"]');
        if (element.isConnected && element.matches(":hover") && !menuOpen && !element.querySelector("[data-hover-label]:hover")) {
          setAnchor(element.getBoundingClientRect());
        }
      }, compact ? 250 : OPEN_DELAY);
    },
    onMouseLeave: compact || !anchor ? hide : leave,
    onPointerDown: hide,
    onContextMenu: hide,
  };
  const card = anchor && createPortal(
    <Card id={id} anchor={anchor} panel={panel} compact={compact} onEnter={stay} onLeave={leave}>{content}</Card>,
    document.body,
  );
  return { handlers, card };
}

function Card({ id, anchor, panel, compact, onEnter, onLeave, children }: {
  id: string;
  anchor: DOMRect;
  panel: React.RefObject<HTMLDivElement | null>;
  compact: boolean;
  onEnter: () => void;
  onLeave: () => void;
  children: ReactNode;
}) {
  const maxWidth = Math.min(WIDTH, window.innerWidth - MARGIN * 2);
  const [labelWidth, setLabelWidth] = useState(0);
  const width = compact ? labelWidth : maxWidth;
  const left = Math.min(Math.max(anchor.left + (anchor.width - width) / 2, MARGIN), window.innerWidth - width - MARGIN);
  const below = Math.max(0, window.innerHeight - anchor.bottom - GAP - MARGIN);
  const above = Math.max(0, anchor.top - GAP - MARGIN);
  const [placeBelow, setPlaceBelow] = useState(below >= above);
  useLayoutEffect(() => {
    if (compact) setLabelWidth(panel.current?.offsetWidth ?? 0);
    const height = panel.current?.scrollHeight ?? 0;
    setPlaceBelow(height <= below || below >= above);
  }, [above, below, children, compact, panel]);
  const position = placeBelow ? { top: anchor.bottom + GAP } : { bottom: window.innerHeight - anchor.top + GAP };
  return (
    <div
      id={id}
      ref={panel}
      role="tooltip"
      style={{ left, width: compact ? "max-content" : width, maxWidth, maxHeight: placeBelow ? below : above, ...position }}
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
      onClick={(event) => event.stopPropagation()}
      onPointerDown={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.stopPropagation()}
      className={compact
        ? "pointer-events-none fixed z-[60] rounded-md border border-line bg-panel px-2 py-1 text-[11px] text-fg-2 shadow-pop motion-safe:animate-fade"
        : "selectable fixed z-[60] animate-fade overflow-auto overscroll-contain rounded-lg border border-line bg-panel px-3 py-2.5 text-left shadow-pop"}
    >
      {children}
    </div>
  );
}

type Paths = { label?: string; values: readonly string[] };

export function SummaryCard({ name, publisher, text, paths, meta }: {
  name: string;
  publisher?: string;
  text?: string;
  paths: readonly Paths[];
  meta?: string;
}) {
  const t = useT();
  return (
    <>
      <div className="text-[13px] font-semibold text-fg [overflow-wrap:anywhere]">{name}</div>
      {publisher && <div className="text-[12px] text-fg-3 [overflow-wrap:anywhere]">{publisher}</div>}
      <p className="m-0 mt-1.5 text-[12px] leading-relaxed whitespace-pre-line text-fg-2 [overflow-wrap:anywhere]">{text || t("summary.notYet")}</p>
      {meta && <div className="mt-1.5 text-[12px] text-fg-3">{meta}</div>}
      {paths.map(({ label, values }) => (
        <section key={label ?? "files"} className="mt-2 border-t border-line pt-2">
          <div className="text-[11px] font-medium text-fg-3">{label ?? t("hover.filePaths")}</div>
          {values.length > 0 ? (
            <ul className="m-0 mt-1 flex list-none flex-col gap-1.5 p-0 font-mono text-[11px] leading-relaxed text-fg-2">
              {values.map((path) => <li key={path} className="[overflow-wrap:anywhere]">{path}</li>)}
            </ul>
          ) : <div className="mt-1 text-[12px] text-fg-3">{t("hover.noPath")}</div>}
        </section>
      ))}
    </>
  );
}

type SoftwareDetails = { files?: readonly PackageFile[]; name?: string; meta?: string };

export function useSoftwareHover(software: Software, details?: SoftwareDetails) {
  return useHoverCard(<SoftwareSummaryCard software={software} {...details} />);
}

function SoftwareSummaryCard({ software, files, name, meta }: { software: Software } & SoftwareDetails) {
  const summary = useApp((state) => state.library?.summaries[software.name]);
  const root = useApp((state) => state.status?.repoRoot);
  const translate = useApp((state) => state.library?.settings.translateSummaries ?? true);
  const language = useLanguage();
  const main = files ? null : primaryVersion(software);
  const packages = files ?? [
    ...(main?.files ?? []),
    ...sortedVersions(software).filter((version) => version !== main).flatMap((version) => version.files),
  ];
  const values = [...new Set(packages.map((file) => fullPath(file, root)).filter(Boolean))];
  return <SummaryCard name={name ?? software.name} publisher={software.publisher} text={shownSummary(summary, language, translate)?.text} meta={meta} paths={[{ values }]} />;
}
