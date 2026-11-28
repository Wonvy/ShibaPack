// 卸载窗口：先分析，再卸载。
//
// 1. 分析（只读）：每个软件列出“App”本身（Windows 是它登记的卸载命令，macOS 是 .app）和它的数据、快捷方式、
//    设置、开机启动项。有把握的放在“待移除”、默认勾上；只是名字像的放在“需确认”、默认不勾。
// 2. 点“卸载”：勾着 App 的先卸载（Windows 运行它自己的卸载程序，等它跑完再查一遍它没删干净的；macOS 移到废纸篓），
//    再清理勾选的项。没勾 App 的只清理数据。一次可以卸载好几个，一个接一个来。
// 3. 结果：清了多少、哪些要管理员权限（可以重试）、注册表备份在哪。
//
// 文件和文件夹移到回收站，注册表项先导出再删；后端删之前按规则再查一遍每一项（leftovers::allowed）。

import { ChevronRight, CircleCheck, FolderOpen, LoaderCircle, ShieldAlert, TriangleAlert } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { type Key, errorText } from "../lib/i18n";
import { formatDate, formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, Leftover, LeftoverScan, RemoveReport } from "../lib/types";
import { SoftwareIcon } from "./SoftwareIcon";
import { Button, IconButton, Modal, Tick, cx } from "./ui";

type Phase = "analyzing" | "ready" | "waiting" | "rescanning" | "removing" | "done" | "cancelled" | "skipped" | "failed";

interface Card {
  app: InstalledApp;
  phase: Phase;
  /** 当前这份清单：卸载前是分析的结果，Windows 卸载程序跑完后换成重新查到的。序号和后端的对得上。 */
  items: Leftover[];
  appPath: string;
  installFolder?: LeftoverScan["installFolder"];
  /** 勾选的项，按 `keyOf`：卸载程序跑完重新查一遍以后序号会变，按路径对。 */
  checked: Set<string>;
  appChecked: boolean;
  open: boolean;
  report: RemoveReport | null;
  /** 清理了的序号（当前清单里的），算腾出的空间、以管理员身份重试用。 */
  removedIndices: number[];
  uninstalled: boolean;
  /** 卸载程序已经删掉的勾选项。 */
  byUninstaller: number;
  error: string | null;
  retrying: boolean;
}

const keyOf = (item: Leftover) => `${item.path}\u0000${item.value ?? ""}`;

/** 跟着软件本身走的项（链接协议、文件类型、防火墙规则）：只在连 App 一起卸载时才能勾，只清数据时删了会让还装着的软件出问题。 */
const appBound = (item: Leftover) => item.reason === "appClass" || item.reason === "firewall" || item.reason === "appPath";
const EMPTY_REPORT: RemoveReport = { removed: 0, failed: [], needsAdmin: false, backup: null };

export function UninstallDialog() {
  const target = useApp((state) => state.uninstallTarget);
  if (!target) return null;
  return <Dialog key={target.apps.map((app) => app.id).join("|")} apps={target.apps} analyze={target.analyze} />;
}

function Dialog({ apps, analyze }: { apps: InstalledApp[]; analyze: boolean }) {
  const t = useT();
  const language = useLanguage();
  const patch = useApp((state) => state.patch);
  const notify = useApp((state) => state.notify);
  const sizes = useApp((state) => state.sizes);
  const appSize = (app: InstalledApp) => sizes[app.id] ?? app.size ?? 0;

  const [cards, setCards] = useState<Card[]>(() =>
    apps.map((app) => ({
      app,
      phase: "analyzing",
      items: [],
      appPath: app.location,
      checked: new Set(),
      appChecked: !analyze && app.uninstallable,
      open: apps.length === 1,
      report: null,
      removedIndices: [],
      uninstalled: false,
      byUninstaller: 0,
      error: null,
      retrying: false,
    })),
  );
  // 一个接一个卸载时要读到最新的勾选
  const latest = useRef(cards);
  latest.current = cards;
  const [stage, setStage] = useState<"choose" | "running" | "finished">("choose");
  // 开始以后按钮上的字不跟着各个软件的进度变
  const [runLabel, setRunLabel] = useState("");
  const waiters = useRef(new Map<string, (outcome: "done" | "skip") => void>());

  const update = (id: string, change: Partial<Card> | ((card: Card) => Partial<Card>)) =>
    setCards((list) => list.map((card) => (card.app.id === id ? { ...card, ...(typeof change === "function" ? change(card) : change) } : card)));

  // 限制并行分析数量，勾选很多软件时也不会同时扫描整台电脑几十次。
  useEffect(() => {
    let active = true;
    let next = 0;
    const analyzeNext = async () => {
      while (active && next < apps.length) {
        const app = apps[next++];
        try {
          const scan = await api.analyzeUninstall(app.id);
          if (!active) return;
          update(app.id, {
            phase: "ready",
            items: scan.items,
            appPath: scan.appPath ?? app.location,
            installFolder: scan.installFolder,
            checked: new Set(
              scan.items.filter((item) => item.confidence !== "low" && (!appBound(item) || (!analyze && app.uninstallable))).map(keyOf),
            ),
          });
        } catch (reason) {
          if (active) update(app.id, { phase: "failed", error: errorText(language, reason), open: true });
        }
      }
    };
    void analyzeNext();
    void analyzeNext();
    const pending = waiters.current;
    return () => {
      active = false;
      pending.forEach((finish) => finish("skip"));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const close = () => {
    if (stage === "running") return;
    patch({ uninstallTarget: null });
  };

  /** 等 Windows 的卸载程序跑完：卸载信息从注册表里没了就算完；也可以点“已经卸载完了”或“不清理了”。 */
  const waitUninstaller = (id: string) =>
    new Promise<"done" | "skip">((resolve) => {
      let over = false;
      const finish = (outcome: "done" | "skip") => {
        if (over) return;
        over = true;
        clearInterval(timer);
        waiters.current.delete(id);
        resolve(outcome);
      };
      waiters.current.set(id, finish);
      const timer = setInterval(() => {
        void api.uninstallStatus(id).then((running) => !running && finish("done"), () => {});
      }, 1500);
    });

  const runOne = async (card: Card) => {
    const id = card.app.id;
    let items = card.items;
    let chosen = items.flatMap((item, i) => (card.checked.has(keyOf(item)) ? [i] : []));
    let byUninstaller = 0;
    if (card.appChecked) {
      let skipCleanup = false;
      try {
        await api.uninstallInstalled(id);
        if (card.app.source === "registry") {
          update(id, { phase: "waiting", open: true });
          skipCleanup = (await waitUninstaller(id)) === "skip";
          if (await api.uninstallStatus(id)) {
            update(id, { phase: "cancelled", open: true });
            return;
          }
        }
      } catch (reason) {
        update(id, { phase: "failed", error: errorText(language, reason), open: true });
        return;
      }
      useApp.getState().removeInstalled(id);
      update(id, { uninstalled: true });
      if (skipCleanup) {
        update(id, { phase: "done", report: null });
        return;
      }
      if (card.app.source === "registry") {
        update(id, { phase: "rescanning" });
        let scan;
        try {
          scan = await api.scanLeftovers(id);
        } catch (reason) {
          update(id, { phase: "failed", error: errorText(language, reason), open: true });
          return;
        }
        if (scan.stillInstalled) {
          update(id, { phase: "cancelled", uninstalled: false, open: true });
          return;
        }
        // 勾过的还在就清；卸载前没有、现在冒出来的（卸载程序没删干净的安装文件夹、卸载项）有把握的也清
        const before = new Set(card.items.map(keyOf));
        const now = new Set(scan.items.map(keyOf));
        byUninstaller = [...card.checked].filter((key) => !now.has(key)).length;
        items = scan.items;
        chosen = items.flatMap((item, i) =>
          card.checked.has(keyOf(item)) || (!before.has(keyOf(item)) && item.confidence !== "low") ? [i] : [],
        );
      }
    }
    update(id, { phase: "removing", items, byUninstaller });
    let report = EMPTY_REPORT;
    if (chosen.length > 0) {
      try {
        report = await api.removeLeftovers(id, chosen);
      } catch (reason) {
        update(id, { phase: "failed", error: errorText(language, reason), open: true });
        return;
      }
    }
    update(id, { phase: "done", report, removedIndices: chosen, open: report.failed.length > 0 || apps.length === 1 });
  };

  const run = async () => {
    setRunLabel(actionLabel);
    setStage("running");
    const pending = latest.current;
    for (const card of pending) {
      if (card.phase === "ready" && included(card)) await runOne(card);
      else if (card.phase === "ready") update(card.app.id, { phase: "skipped" });
    }
    setStage("finished");
    if (pending.some((card) => card.phase === "ready" && card.appChecked)) void useApp.getState().loadInstalled();
  };

  const retryAdmin = async (card: Card) => {
    const failed = failedIndices(card);
    update(card.app.id, { retrying: true });
    try {
      const next = await api.removeLeftovers(card.app.id, failed, true);
      update(card.app.id, (current) => ({
        report: {
          removed: (current.report?.removed ?? 0) + next.removed,
          failed: next.failed,
          needsAdmin: next.needsAdmin,
          backup: next.backup ?? current.report?.backup ?? null,
        },
      }));
    } catch (reason) {
      notify(errorText(language, reason), "error");
    } finally {
      update(card.app.id, { retrying: false });
    }
  };

  // 底部：勾了几项、多大；按钮写“卸载”还是“只清理数据”
  const ready = cards.filter((card) => card.phase === "ready");
  const analyzing = cards.some((card) => card.phase === "analyzing");
  const chosenCount = ready.reduce((sum, card) => sum + card.checked.size + (card.appChecked ? 1 : 0), 0);
  const chosenSize = ready.reduce((sum, card) => sum + pickedSize(card, appSize(card.app)), 0);
  const uninstalling = ready.filter((card) => card.appChecked).length;
  const anything = ready.some(included);
  const actionLabel =
    uninstalling === 0 ? t("uninstall.cleanData") : apps.length === 1 ? t("installed.uninstall") : t("uninstall.uninstallN", { n: uninstalling });
  const windows = apps.some((app) => app.source === "registry");

  const footer =
    stage === "finished" ? (
      <Button variant="primary" onClick={close}>
        {t("leftover.finish")}
      </Button>
    ) : (
      <>
        <span className="mr-auto text-[12px] text-fg-3">
          {stage === "running"
            ? t("uninstall.working")
            : chosenCount > 0
              ? t("uninstall.selected", { n: chosenCount, size: formatSize(chosenSize) })
              : t("uninstall.selectedNone")}
        </span>
        <Button variant="ghost" disabled={stage === "running"} onClick={close}>
          {t("common.cancel")}
        </Button>
        <Button variant="danger" busy={stage === "running"} disabled={analyzing || !anything || stage === "running"} onClick={() => void run()}>
          {stage === "running" ? runLabel : actionLabel}
        </Button>
      </>
    );

  const title = analyze ? t("uninstall.analyzeTitle") : apps.length > 1 ? t("uninstall.titleMany", { n: apps.length }) : t("installed.uninstall");

  return (
    <Modal open onClose={close} title={title} width={760} footer={footer}>
      <div className="-mx-2 flex max-h-[62vh] flex-col gap-2 overflow-y-auto px-2">
        {cards.map((card) => (
          <AppCard
            key={card.app.id}
            card={card}
            appSize={appSize(card.app)}
            locked={stage !== "choose"}
            onChange={(change) => update(card.app.id, change)}
            onResolve={(outcome) => waiters.current.get(card.app.id)?.(outcome)}
            onRetry={() => void retryAdmin(card)}
          />
        ))}
      </div>
      {stage === "choose" && (
        <div className="mt-3 space-y-1 text-[12px] leading-relaxed text-fg-3">
          <p className="m-0">{t(windows ? "uninstall.note.windows" : "uninstall.note.mac")}</p>
          <p className="m-0">{t("uninstall.previewScope")}</p>
        </div>
      )}
    </Modal>
  );
}

function included(card: Card): boolean {
  return card.appChecked || card.checked.size > 0;
}

function pickedSize(card: Card, appSize: number): number {
  const data = card.items.reduce((sum, item) => sum + (card.checked.has(keyOf(item)) ? (item.size ?? 0) : 0), 0);
  return data + (card.appChecked ? appSize : 0);
}

/** 没清掉的项在当前清单里的序号。 */
function failedIndices(card: Card): number[] {
  return (card.report?.failed ?? []).map((f) => card.items.findIndex((item) => keyOf(item) === keyOf(f.item))).filter((i) => i >= 0);
}

/** 一个软件：标题行（图标、名字、版本、大小、日期，右边是要移除多少、折叠、全选），下面是清单或结果。 */
function AppCard({
  card,
  appSize,
  locked,
  onChange,
  onResolve,
  onRetry,
}: {
  card: Card;
  appSize: number;
  locked: boolean;
  onChange: (change: Partial<Card>) => void;
  onResolve: (outcome: "done" | "skip") => void;
  onRetry: () => void;
}) {
  const t = useT();
  const language = useLanguage();
  const { app } = card;
  const rows = card.items.map((item, index) => ({ item, index }));
  const sure = rows.filter(({ item }) => item.confidence !== "low");
  const unsure = rows.filter(({ item }) => item.confidence === "low");
  const total = card.items.length + (app.uninstallable ? 1 : 0);
  const picked = card.checked.size + (card.appChecked ? 1 : 0);
  const all: boolean | "mixed" = picked === 0 ? false : picked === total ? true : "mixed";
  const editable = card.phase === "ready" && !locked;

  const toggle = (item: Leftover, on: boolean) => {
    const next = new Set(card.checked);
    if (on) next.add(keyOf(item));
    else next.delete(keyOf(item));
    onChange({ checked: next });
  };
  const setAll = (on: boolean) =>
    onChange({
      appChecked: on && app.uninstallable,
      checked: new Set(on ? card.items.filter((item) => app.uninstallable || !appBound(item)).map(keyOf) : []),
    });
  /** 勾上 App 时把跟着它的项一起勾上；取消 App 时这些项也取消。 */
  const toggleApp = (on: boolean) => {
    const next = new Set(card.checked);
    for (const item of card.items.filter(appBound)) {
      if (on && item.confidence !== "low") next.add(keyOf(item));
      else if (!on) next.delete(keyOf(item));
    }
    onChange({ appChecked: on, checked: next });
  };
  const locks = (item: Leftover) => appBound(item) && !card.appChecked;
  const unsureOpen = unsure.filter(({ item }) => !locks(item));
  const unsureAll = unsureOpen.length > 0 && unsureOpen.every(({ item }) => card.checked.has(keyOf(item)));

  const removed = card.removedIndices.filter((i) => !card.report?.failed.some((f) => keyOf(f.item) === keyOf(card.items[i]!)));
  const freed = removed.reduce((sum, i) => sum + (card.items[i]?.size ?? 0), 0) + (card.uninstalled ? appSize : 0);

  const status = (() => {
    switch (card.phase) {
      case "analyzing":
        return <Busy text={t("uninstall.analyzing")} />;
      case "ready":
        return <span className="tabular">{t("uninstall.removeSize", { size: formatSize(pickedSize(card, appSize)) })}</span>;
      case "waiting":
        return <Busy text={t("uninstall.waiting")} />;
      case "rescanning":
        return <Busy text={t("uninstall.rescanning")} />;
      case "removing":
        return <Busy text={t("leftover.removing")} />;
      case "done":
        return (
          <span className="flex items-center gap-1.5 text-success">
            <CircleCheck size={14} strokeWidth={1.75} aria-hidden />
            {t("uninstall.done")}
          </span>
        );
      case "cancelled":
        return <span className="text-warning">{t("uninstall.cancelledShort")}</span>;
      case "failed":
        return <span className="text-danger">{t("uninstall.failedShort")}</span>;
      case "skipped":
        return <span>{t("uninstall.notIncluded")}</span>;
    }
  })();

  return (
    <section className="rounded-lg border border-line">
      <div
        role="button"
        tabIndex={0}
        aria-expanded={card.open}
        onClick={() => onChange({ open: !card.open })}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onChange({ open: !card.open });
          }
        }}
        className="flex items-center gap-3 rounded-lg px-3 py-2.5 transition-colors hover:bg-hover"
      >
        <SoftwareIcon name={app.name} icon={app.icon} size={32} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold">
            {app.name} {app.version && <span className="font-normal text-fg-3">{app.version}</span>}
          </div>
          <div className="truncate text-[12px] text-fg-3">
            {[app.publisher, appSize > 0 ? formatSize(appSize) : "", app.installedAt ? formatDate(app.installedAt, language) : ""].filter(Boolean).join(" · ")}
          </div>
        </div>
        <span className="shrink-0 text-[12px] text-fg-3">{status}</span>
        <ChevronRight size={14} strokeWidth={1.75} aria-hidden className={cx("shrink-0 text-fg-3 transition-transform", card.open && "rotate-90")} />
        {card.phase === "ready" && (
          <Tick checked={all} disabled={!editable || total === 0} label={t(all === true ? "leftover.selectNone" : "leftover.selectAll")} onChange={setAll} />
        )}
      </div>

      {card.open && (
        <div className="border-t border-line px-1.5 pt-1 pb-2">
          {card.phase === "ready" && (
            <>
              {card.installFolder && card.installFolder.path !== card.appPath && (
                <div className="flex items-start gap-3 px-2 pt-2 text-[12px] text-fg-3">
                  <span className="shrink-0">{t(card.installFolder.inferred ? "uninstall.installFolderInferred" : "uninstall.installFolder")}</span>
                  <span className="selectable min-w-0 break-all font-mono text-fg-2" title={card.installFolder.path}>
                    {shortPath(card.installFolder.path)}
                  </span>
                </div>
              )}
              {app.source === "registry" && !card.installFolder && <Empty>{t("uninstall.installFolderUnknown")}</Empty>}
              <Heading>{t("uninstall.toRemove")}</Heading>
              {app.uninstallable && (
                <Row
                  checked={card.appChecked}
                  disabled={!editable}
                  kind={t(app.source === "registry" ? "uninstall.kind.command" : "uninstall.kind.app")}
                  path={card.appPath}
                  size={appSize}
                  onToggle={toggleApp}
                />
              )}
              {sure.map(({ item, index }) => (
                <Row
                  key={keyOf(item)}
                  checked={card.checked.has(keyOf(item))}
                  disabled={!editable || locks(item)}
                  kind={t(kindKey(item))}
                  path={label(item)}
                  why={locks(item) ? `${t(`leftover.reason.${item.reason}` as Key)}\n${t("uninstall.appBoundHint")}` : t(`leftover.reason.${item.reason}` as Key)}
                  size={item.size}
                  onToggle={(on) => toggle(item, on)}
                  onReveal={revealable(item) ? () => void api.revealLeftover(app.id, index) : undefined}
                />
              ))}
              {sure.length === 0 && !app.uninstallable && <Empty>{t("uninstall.noData")}</Empty>}
              {unsure.length > 0 && (
                <>
                  <Heading
                    action={
                      editable && (
                        <button
                          type="button"
                          className="text-[12px] font-normal text-accent hover:underline"
                          onClick={() => {
                            const next = new Set(card.checked);
                            for (const { item } of unsureOpen) {
                              if (unsureAll) next.delete(keyOf(item));
                              else next.add(keyOf(item));
                            }
                            onChange({ checked: next });
                          }}
                        >
                          {t(unsureAll ? "leftover.selectNone" : "leftover.selectAll")}
                        </button>
                      )
                    }
                  >
                    {t("uninstall.toConfirm")}
                  </Heading>
                  {unsure.map(({ item, index }) => (
                    <Row
                      key={keyOf(item)}
                      checked={card.checked.has(keyOf(item))}
                      disabled={!editable || locks(item)}
                      kind={t(kindKey(item))}
                      path={label(item)}
                      why={locks(item) ? `${t(`leftover.reason.${item.reason}` as Key)}\n${t("uninstall.appBoundHint")}` : t(`leftover.reason.${item.reason}` as Key)}
                      size={item.size}
                      onToggle={(on) => toggle(item, on)}
                      onReveal={revealable(item) ? () => void api.revealLeftover(app.id, index) : undefined}
                    />
                  ))}
                </>
              )}
              {card.items.length === 0 && app.uninstallable && <Empty>{t("uninstall.noData")}</Empty>}
              {!card.appChecked && card.checked.size > 0 && (
                <p className="m-0 mx-2 mt-2 flex items-start gap-1.5 text-[12px] leading-relaxed text-warning">
                  <TriangleAlert size={13} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
                  {t("uninstall.dataOnlyHint", { name: app.name })}
                </p>
              )}
            </>
          )}

          {card.phase === "waiting" && (
            <div className="px-2 py-1.5">
              <p className="m-0 text-[12px] leading-relaxed text-fg-3">{t("uninstall.waitingHint")}</p>
              <div className="mt-2 flex gap-2">
                <Button size="sm" onClick={() => onResolve("done")}>
                  {t("uninstall.doneWaiting")}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => onResolve("skip")}>
                  {t("leftover.skipCleanup")}
                </Button>
              </div>
            </div>
          )}

          {card.phase === "cancelled" && <Empty tone="warning">{t("uninstall.cancelled", { name: app.name })}</Empty>}
          {card.phase === "failed" && (
            <>
              {card.uninstalled && <Empty>{t("uninstall.resultUninstalled", { name: app.name })}</Empty>}
              <Empty tone="danger">{card.error}</Empty>
            </>
          )}
          {card.phase === "skipped" && <Empty>{t("uninstall.skippedHint")}</Empty>}

          {card.phase === "done" && (
            <div className="flex flex-col gap-1.5 px-2 py-1.5 text-[13px] text-fg-2">
              {card.uninstalled && <p className="m-0">{t("uninstall.resultUninstalled", { name: app.name })}</p>}
              {card.byUninstaller > 0 && <p className="m-0 text-[12px] text-fg-3">{t("uninstall.resultByUninstaller", { n: card.byUninstaller })}</p>}
              {card.report === null ? (
                <p className="m-0 text-fg-3">{t("uninstall.resultSkipped")}</p>
              ) : removed.length > 0 ? (
                <p className="m-0">{t("uninstall.resultRemoved", { n: removed.length, size: formatSize(freed) })}</p>
              ) : (
                card.report.failed.length === 0 && <p className="m-0 text-fg-3">{t("uninstall.resultNothing")}</p>
              )}
              {card.report && card.report.failed.length > 0 && (
                <>
                  <p className="m-0 text-[12px] text-warning">
                    {t(card.report.needsAdmin ? "leftover.failedAdmin" : "leftover.failed", { n: card.report.failed.length })}
                  </p>
                  <div className="rounded-md border border-line">
                    {card.report.failed.map((failure, i) => (
                      <div key={i} className="flex items-center gap-3 border-b border-line px-2.5 py-1.5 last:border-b-0">
                        <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px]" title={label(failure.item)}>
                          {shortPath(label(failure.item))}
                        </span>
                        <span className="shrink-0 text-[12px] text-fg-3">{failureText(t, failure.reason)}</span>
                      </div>
                    ))}
                  </div>
                  {card.report.needsAdmin && (
                    <div>
                      <Button size="sm" busy={card.retrying} onClick={onRetry}>
                        <ShieldAlert size={13} strokeWidth={1.75} aria-hidden />
                        {t("leftover.retryAdmin")}
                      </Button>
                    </div>
                  )}
                </>
              )}
              {card.report?.backup && (
                <p className="m-0 flex items-center gap-2 text-[12px] text-fg-3">
                  <span className="min-w-0 flex-1 truncate" title={card.report.backup}>
                    {t("leftover.backup", { path: card.report.backup })}
                  </span>
                  <Button variant="ghost" size="sm" onClick={() => void api.openUninstallBackup(card.report!.backup!)}>
                    <FolderOpen size={13} strokeWidth={1.75} aria-hidden />
                    {t("leftover.openBackup")}
                  </Button>
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

function Heading({ children, action }: { children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className="mt-2 mb-0.5 flex h-6 items-center px-2 text-[12px] font-semibold text-fg-2">
      {children}
      <span className="ml-auto">{action}</span>
    </div>
  );
}

function Empty({ children, tone }: { children: React.ReactNode; tone?: "warning" | "danger" }) {
  return (
    <p className={cx("m-0 px-2 py-1.5 text-[12px] leading-relaxed", tone === "warning" ? "text-warning" : tone === "danger" ? "text-danger" : "text-fg-3")}>
      {children}
    </p>
  );
}

/** 清单里的一行：勾选框、类型、路径（家目录写成 ~）、在文件夹中显示、大小。点整行也能勾。 */
function Row({
  checked,
  disabled,
  kind,
  path,
  why,
  size,
  onToggle,
  onReveal,
}: {
  checked: boolean;
  disabled: boolean;
  kind: string;
  path: string;
  why?: string;
  size: number | null;
  onToggle: (on: boolean) => void;
  onReveal?: () => void;
}) {
  const t = useT();
  return (
    <div
      onClick={() => !disabled && onToggle(!checked)}
      className={cx("group/row flex h-8 items-center gap-3 rounded-md px-2 transition-colors", !disabled && "hover:bg-hover")}
    >
      <Tick checked={checked} disabled={disabled} label={kind} onChange={onToggle} />
      <span className="w-[76px] shrink-0 truncate text-[13px] font-medium">{kind}</span>
      <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px] text-fg-2" title={why ? `${path}\n${why}` : path}>
        {shortPath(path)}
      </span>
      {onReveal && (
        <IconButton
          label={t("uninstall.reveal")}
          className="size-6 opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100"
          onClick={(event) => {
            event.stopPropagation();
            onReveal();
          }}
        >
          <FolderOpen size={13} strokeWidth={1.75} aria-hidden />
        </IconButton>
      )}
      <span className="tabular w-16 shrink-0 text-right text-[12px] text-fg-3">{size !== null && size > 0 ? formatSize(size) : ""}</span>
    </div>
  );
}

function Busy({ text }: { text: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
      {text}
    </span>
  );
}

/** 这一项是什么：快捷方式、缓存、日志、偏好设置、注册表、开机启动，其余算 App 数据。 */
function kindKey(item: Leftover): Key {
  if (item.reason === "installDir") return "uninstall.kind.installDir";
  if (item.reason === "uninstallKey") return "uninstall.kind.uninstallKey";
  if (item.reason === "shortcut") return "uninstall.kind.shortcut";
  if (item.reason === "usage") return "uninstall.kind.usage";
  if (item.reason === "firewall") return "uninstall.kind.firewall";
  if (item.kind === "registryValue" || /[\\/]LaunchAgents[\\/]/.test(item.path)) return "uninstall.kind.startup";
  if (item.kind === "registryKey") return "uninstall.kind.registry";
  if (/[\\/]Preferences[\\/]/.test(item.path)) return "uninstall.kind.prefs";
  if (/[\\/](Caches?|Code Cache|GPUCache)([\\/]|$)/i.test(item.path)) return "uninstall.kind.cache";
  if (/[\\/]Logs?[\\/]/i.test(item.path)) return "uninstall.kind.logs";
  return "uninstall.kind.data";
}

function revealable(item: Leftover): boolean {
  return item.kind === "folder" || item.kind === "file";
}

/** 一项残留在列表里显示的样子：路径；注册表值写成“键 → 值名”。 */
function label(item: Leftover): string {
  return item.value ? `${item.path} → ${item.value}` : item.path;
}

/** 用户文件夹写成 ~，路径短一些（悬停能看到完整的）。 */
function shortPath(path: string): string {
  return path.replace(/^(?:[A-Za-z]:\\Users\\[^\\]+|\/Users\/[^/]+|\/home\/[^/]+)(?=[\\/])/, "~");
}

function failureText(t: ReturnType<typeof useT>, reason: string): string {
  const key = `leftover.fail.${reason}` as Key;
  const text = t(key);
  return text === key ? reason : text;
}
