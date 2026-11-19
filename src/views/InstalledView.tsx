// 已安装软件：这台电脑上装着的程序，以及右侧的详情栏。

import { ArrowDown, ArrowUp, FolderOpen, LoaderCircle, MonitorCheck, MousePointerClick, PackageCheck, PackageX, RefreshCw, SearchX, Trash2 } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";
import { DetailToggle } from "../components/chrome";
import { SoftwareIcon } from "../components/SoftwareIcon";
import { Button, Chip, EmptyState, IconButton, cx } from "../components/ui";
import { uninstallApp } from "../lib/actions";
import { api } from "../lib/api";
import { compareVersions, formatDate, formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp } from "../lib/types";

type Column = "name" | "version" | "date" | "size";
const COLUMNS = "grid grid-cols-[minmax(0,1fr)_auto] @lg:grid-cols-[minmax(0,1fr)_110px_80px] @2xl:grid-cols-[minmax(0,1fr)_120px_92px_80px] items-center gap-x-3";

export function InstalledView() {
  const t = useT();
  const language = useLanguage();
  const installed = useApp((state) => state.installed);
  const loading = useApp((state) => state.installedLoading);
  const sizes = useApp((state) => state.sizes);
  const selectedId = useApp((state) => state.selectedInstalledId);
  const patch = useApp((state) => state.patch);
  const reload = useApp((state) => state.loadInstalled);
  const search = useApp((state) => state.search);
  const [sort, setSort] = useState<{ column: Column; asc: boolean }>({ column: "name", asc: true });

  const sizeOf = (app: InstalledApp) => app.size ?? sizes[app.id] ?? null;

  const list = useMemo(() => {
    const query = search.trim().toLowerCase();
    const items = (installed ?? []).filter(
      (app) => !query || app.name.toLowerCase().includes(query) || app.publisher.toLowerCase().includes(query),
    );
    const byName = (a: InstalledApp, b: InstalledApp) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    const compare: Record<Column, (a: InstalledApp, b: InstalledApp) => number> = {
      name: byName,
      version: (a, b) => compareVersions(a.version, b.version) || byName(a, b),
      date: (a, b) => a.installedAt.localeCompare(b.installedAt) || byName(a, b),
      size: (a, b) => (a.size ?? sizes[a.id] ?? -1) - (b.size ?? sizes[b.id] ?? -1) || byName(a, b),
    };
    return items.sort((a, b) => compare[sort.column](a, b) * (sort.asc ? 1 : -1));
  }, [installed, search, sort, sizes]);

  const header = (column: Column, label: string, className?: string) => {
    const Arrow = sort.asc ? ArrowUp : ArrowDown;
    return (
      <button
        type="button"
        onClick={() => setSort({ column, asc: sort.column === column ? !sort.asc : column === "name" })}
        className={cx("flex items-center gap-1 text-left hover:text-fg-2", className)}
      >
        {label}
        {sort.column === column && <Arrow size={12} strokeWidth={2} aria-hidden />}
      </button>
    );
  };

  return (
    <div className="@container flex min-h-0 flex-1 flex-col">
      <div className="flex h-11 shrink-0 items-center gap-1 border-b border-line pr-2 pl-4">
        <h2 className="m-0 min-w-0 truncate text-[14px] font-semibold">{t("installed.title")}</h2>
        <span className="tabular ml-1.5 shrink-0 text-[12px] text-fg-3">{list.length}</span>
        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          <IconButton label={t("installed.refresh")} disabled={loading} onClick={() => void reload()}>
            <RefreshCw size={15} strokeWidth={1.75} className={loading ? "animate-spin" : ""} />
          </IconButton>
          <span className="mx-1 h-4 w-px bg-line-strong" />
          <DetailToggle />
        </div>
      </div>

      {installed === null ? (
        <EmptyState icon={LoaderCircle} title={t("installed.loading")} />
      ) : list.length === 0 ? (
        <EmptyState icon={search.trim() ? SearchX : MonitorCheck} title={search.trim() ? t("lib.noMatch") : t("installed.empty")} />
      ) : (
        <>
          <div className={cx(COLUMNS, "h-8 shrink-0 border-b border-line pr-5 pl-[58px] text-[12px] text-fg-3")}>
            {header("name", t("installed.col.name"))}
            {header("version", t("installed.col.version"))}
            {header("date", t("installed.col.date"), "hidden justify-end @2xl:flex")}
            {header("size", t("installed.col.size"), "hidden justify-end @lg:flex")}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-2 pt-1 pb-3">
            {list.map((app) => {
              const size = sizeOf(app);
              return (
                <div
                  key={app.id}
                  role="button"
                  tabIndex={0}
                  aria-pressed={app.id === selectedId}
                  onClick={() => patch({ selectedInstalledId: app.id })}
                  onKeyDown={(event) => event.key === "Enter" && patch({ selectedInstalledId: app.id })}
                  className={cx(COLUMNS, "min-h-[52px] rounded-md py-1.5 pr-3.5 pl-2.5 transition-colors", app.id === selectedId ? "bg-active" : "hover:bg-hover")}
                >
                  <div className="flex min-w-0 items-center gap-2.5">
                    <SoftwareIcon name={app.name} icon={app.icon} />
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-medium">{app.name}</div>
                      <div className="truncate text-[12px] text-fg-3">{app.publisher || t(`source.${app.source}`)}</div>
                    </div>
                  </div>
                  <div className="tabular truncate text-[12px] text-fg-2">{app.version}</div>
                  <div className="tabular hidden text-right text-[12px] text-fg-3 @2xl:block">{app.installedAt && formatDate(app.installedAt, language)}</div>
                  <div className="tabular hidden text-right text-[12px] text-fg-2 @lg:block">{size === null ? "" : formatSize(size)}</div>
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}

export function InstalledDetail() {
  const t = useT();
  const language = useLanguage();
  const app = useApp((state) => state.installed?.find((item) => item.id === state.selectedInstalledId));
  const measured = useApp((state) => (app ? state.sizes[app.id] : undefined));
  const measuring = useApp((state) => Object.keys(state.sizes).length === 0);
  // 仓库里和它对应的软件
  const linked = useApp((state) => {
    const softwareId = app && Object.keys(state.links).find((id) => state.links[id] === app.id);
    return softwareId ? state.library?.software.find((s) => s.id === softwareId) : undefined;
  });
  const attempt = useApp((state) => state.attempt);
  const setRoute = useApp((state) => state.setRoute);
  const select = useApp((state) => state.select);

  if (!app) return <EmptyState icon={MousePointerClick} title={t("installed.select")} />;

  const size = app.size ?? measured ?? null;
  const showInLibrary = () => {
    if (!linked) return;
    select(linked.id);
    setRoute("library");
  };

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-4">
        <div className="flex items-center gap-3">
          <SoftwareIcon name={app.name} icon={app.icon} size={44} />
          <div className="min-w-0">
            <div className="selectable truncate text-[15px] font-semibold">{app.name}</div>
            <div className="truncate text-[12px] text-fg-2">{app.publisher}</div>
          </div>
        </div>

        <div
          className={cx(
            "mt-4 flex items-center gap-2 rounded-lg px-2.5 py-2 text-[12px]",
            linked ? "bg-accent-soft text-accent" : "bg-hover text-fg-3",
          )}
        >
          {linked ? <PackageCheck size={15} strokeWidth={1.75} aria-hidden /> : <PackageX size={15} strokeWidth={1.75} aria-hidden />}
          <span className="min-w-0 flex-1">{linked ? t("installed.inLibrary") : t("installed.notInLibrary")}</span>
          {linked && (
            <button type="button" onClick={showInLibrary} className="shrink-0 font-medium underline-offset-2 hover:underline">
              {t("installed.showInLibrary")}
            </button>
          )}
        </div>

        <dl className="mt-4 mb-0">
          <Fact label={t("installed.version")}>{app.version || t("installed.unknown")}</Fact>
          <Fact label={t("installed.source")}>
            <Chip>{t(`source.${app.source}`)}</Chip>
          </Fact>
          <Fact label={t("installed.date")}>{app.installedAt ? formatDate(app.installedAt, language) : t("installed.unknown")}</Fact>
          <Fact label={t("installed.size")}>
            {size !== null ? formatSize(size) : measuring ? t("installed.measuring") : t("installed.unknown")}
          </Fact>
          <Fact label={t("installed.location")}>
            <span className="selectable font-mono text-[12px] break-all text-fg-2">{app.location || t("installed.unknown")}</span>
          </Fact>
        </dl>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-t border-line p-3">
        <div className="flex gap-2">
          <Button className="flex-1" disabled={!app.location} onClick={() => void attempt(() => api.revealInstalled(app.id))}>
            <FolderOpen size={14} strokeWidth={1.75} aria-hidden />
            {t("installed.reveal")}
          </Button>
          <Button className="flex-1 text-danger" disabled={!app.uninstallable} onClick={() => void uninstallApp(app)}>
            <Trash2 size={14} strokeWidth={1.75} aria-hidden />
            {t("installed.uninstall")}
          </Button>
        </div>
        {!app.uninstallable && (
          <div className="text-center text-[12px] text-fg-3">{app.source === "system" ? t("installed.protected") : t("installed.noUninstall")}</div>
        )}
      </div>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-start gap-3 border-b border-line py-2 text-[13px] last:border-b-0">
      <dt className="w-[72px] shrink-0 text-[12px] leading-[19px] text-fg-3">{label}</dt>
      <dd className="tabular m-0 min-w-0 flex-1">{children}</dd>
    </div>
  );
}
