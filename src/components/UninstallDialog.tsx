// 卸载窗口：确认 → 运行卸载程序（Windows）或移到废纸篓（macOS）→ 查找残留 → 勾选后清理。
//
// 清理时文件和文件夹移到回收站，注册表项先导出成 .reg 再删；默认只勾高、中可信度的项。
// 软件看起来还没卸掉时（卸载程序被取消了）一项都不默认勾选。

import { CircleCheck, Database, File, Folder, FolderOpen, LoaderCircle, ShieldAlert, TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { type Key, errorText } from "../lib/i18n";
import { formatSize } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { InstalledApp, Leftover, RemoveReport } from "../lib/types";
import { SoftwareIcon } from "./SoftwareIcon";
import { Button, Checkbox, Modal, cx } from "./ui";

type Step = "confirm" | "running" | "scanning" | "results" | "removing" | "done";

export function UninstallDialog() {
  const target = useApp((state) => state.uninstallTarget);
  if (!target) return null;
  return <Dialog key={target.id} target={target} />;
}

function Dialog({ target }: { target: InstalledApp }) {
  const t = useT();
  const language = useLanguage();
  const patch = useApp((state) => state.patch);
  const notify = useApp((state) => state.notify);
  const loadInstalled = useApp((state) => state.loadInstalled);
  // Windows 上是运行软件自己的卸载程序；macOS 上是把 .app 移到废纸篓
  const viaUninstaller = target.source === "registry";

  const [step, setStep] = useState<Step>("confirm");
  const [cleanup, setCleanup] = useState(true);
  const [items, setItems] = useState<Leftover[]>([]);
  const [stillInstalled, setStillInstalled] = useState(false);
  const [checked, setChecked] = useState<Set<number>>(new Set());
  const [report, setReport] = useState<RemoveReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  // 卸载已经开始了：关窗口时要重新列一遍已安装软件
  const [started, setStarted] = useState(false);

  const close = () => {
    patch({ uninstallTarget: null });
    if (started) void loadInstalled();
  };

  const scan = async () => {
    setStep("scanning");
    try {
      const result = await api.scanLeftovers(target.id);
      setItems(result.items);
      setStillInstalled(result.stillInstalled);
      // 默认勾选高、中可信度的；软件还在时一项都不勾
      setChecked(new Set(result.stillInstalled ? [] : result.items.flatMap((item, i) => (item.confidence === "low" ? [] : [i]))));
      setStep("results");
    } catch (reason) {
      setError(errorText(language, reason));
      setStep("results");
    }
  };

  const start = async () => {
    try {
      await api.uninstallInstalled(target.id);
    } catch (reason) {
      notify(errorText(language, reason), "error");
      close();
      return;
    }
    setStarted(true);
    if (!cleanup) {
      notify(t(viaUninstaller ? "installed.uninstallStarted" : "installed.uninstalled"));
      patch({ uninstallTarget: null });
      void loadInstalled();
      return;
    }
    if (viaUninstaller) setStep("running");
    else void scan();
  };

  // 等卸载程序跑完：卸载信息从注册表里消失了就去找残留
  useEffect(() => {
    if (step !== "running") return;
    let alive = true;
    const timer = setInterval(() => {
      void api.uninstallStatus(target.id).then((running) => {
        if (alive && !running) {
          alive = false;
          void scan();
        }
      });
    }, 1500);
    return () => {
      alive = false;
      clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  const remove = async (indices: number[], elevated: boolean) => {
    if (elevated) setRetrying(true);
    else setStep("removing");
    try {
      const next = await api.removeLeftovers(target.id, indices, elevated);
      // 以管理员身份重试的结果并到上一次的结果里
      setReport((previous) =>
        elevated && previous
          ? { removed: previous.removed + next.removed, failed: next.failed, needsAdmin: next.needsAdmin, backup: next.backup ?? previous.backup }
          : next,
      );
    } catch (reason) {
      notify(errorText(language, reason), "error");
    } finally {
      setRetrying(false);
      setStep("done");
    }
  };

  const chosen = [...checked].sort((a, b) => a - b);
  const chosenSize = chosen.reduce((sum, i) => sum + (items[i]?.size ?? 0), 0);
  const failedIndices = useMemo(
    () => (report?.failed ?? []).map((f) => items.findIndex((item) => item.path === f.item.path && item.value === f.item.value)).filter((i) => i >= 0),
    [report, items],
  );

  const footer = (() => {
    switch (step) {
      case "confirm":
        return (
          <>
            <Button variant="ghost" onClick={close}>
              {t("common.cancel")}
            </Button>
            <Button variant="danger" onClick={() => void start()}>
              {t("installed.uninstall")}
            </Button>
          </>
        );
      case "running":
        return (
          <>
            <Button variant="ghost" className="mr-auto" onClick={close}>
              {t("leftover.skipCleanup")}
            </Button>
            <Button onClick={() => void scan()}>{t("leftover.scanNow")}</Button>
          </>
        );
      case "results":
        return items.length === 0 ? (
          <Button variant="primary" onClick={close}>
            {t("leftover.finish")}
          </Button>
        ) : (
          <>
            <Button variant="ghost" onClick={close}>
              {t("leftover.skip")}
            </Button>
            <Button variant="danger" disabled={chosen.length === 0} onClick={() => void remove(chosen, false)}>
              {t("leftover.remove", { n: chosen.length })}
            </Button>
          </>
        );
      case "done":
        return (
          <>
            {report?.needsAdmin && failedIndices.length > 0 && (
              <Button className="mr-auto" busy={retrying} onClick={() => void remove(failedIndices, true)}>
                <ShieldAlert size={14} strokeWidth={1.75} aria-hidden />
                {t("leftover.retryAdmin")}
              </Button>
            )}
            <Button variant="primary" onClick={close}>
              {t("leftover.finish")}
            </Button>
          </>
        );
      default:
        return null;
    }
  })();

  return (
    <Modal open onClose={close} title={t("installed.uninstall")} width={660} footer={footer}>
      <div className="mb-3 flex items-center gap-3">
        <SoftwareIcon name={target.name} icon={target.icon} size={32} />
        <div className="min-w-0">
          <div className="truncate text-[13px] font-medium">{target.name}</div>
          <div className="truncate text-[12px] text-fg-3">{[target.version, target.publisher].filter(Boolean).join(" · ")}</div>
        </div>
      </div>

      {step === "confirm" && (
        <>
          <p className="m-0 text-[13px] leading-relaxed text-fg-2">
            {t(viaUninstaller ? "installed.uninstallConfirm.run" : "installed.uninstallConfirm.trash", { name: target.name })}
          </p>
          <div className="mt-3">
            <Checkbox checked={cleanup} onChange={setCleanup}>
              <span className="text-[13px]">{t("leftover.offer")}</span>
            </Checkbox>
            <p className="m-0 mt-1 ml-6 text-[12px] leading-relaxed text-fg-3">{t("leftover.offerHint")}</p>
          </div>
        </>
      )}

      {step === "running" && (
        <Busy text={t("leftover.waiting", { name: target.name })} hint={t("leftover.waitingHint")} />
      )}
      {step === "scanning" && <Busy text={t("leftover.scanning")} />}
      {step === "removing" && <Busy text={t("leftover.removing")} />}

      {step === "results" && error && <p className="m-0 text-[13px] text-danger">{error}</p>}
      {step === "results" && !error && items.length === 0 && (
        <p className="m-0 flex items-center gap-2 text-[13px] text-fg-2">
          <CircleCheck size={15} strokeWidth={1.75} aria-hidden className="text-success" />
          {t("leftover.none", { name: target.name })}
        </p>
      )}
      {step === "results" && items.length > 0 && (
        <>
          {stillInstalled && (
            <p className="m-0 mb-2 flex items-start gap-2 rounded-md bg-hover px-2.5 py-2 text-[12px] leading-relaxed text-warning">
              <TriangleAlert size={14} strokeWidth={1.75} aria-hidden className="mt-0.5 shrink-0" />
              {t("leftover.stillInstalled", { name: target.name })}
            </p>
          )}
          <p className="m-0 mb-2 text-[12px] leading-relaxed text-fg-3">
            {t(viaUninstaller ? "leftover.summaryWindows" : "leftover.summaryMac", { n: items.length })}
          </p>
          <LeftoverList
            items={items}
            checked={checked}
            onToggle={(i, on) => {
              const next = new Set(checked);
              if (on) next.add(i);
              else next.delete(i);
              setChecked(next);
            }}
          />
          <div className="mt-2 flex items-center justify-between text-[12px] text-fg-3">
            <span>{chosenSize > 0 ? t("leftover.selectedSize", { size: formatSize(chosenSize) }) : ""}</span>
            <span className="flex gap-2">
              <button type="button" className="hover:text-fg-2" onClick={() => setChecked(new Set(items.map((_, i) => i)))}>
                {t("leftover.selectAll")}
              </button>
              <button type="button" className="hover:text-fg-2" onClick={() => setChecked(new Set())}>
                {t("leftover.selectNone")}
              </button>
            </span>
          </div>
        </>
      )}

      {step === "done" && report && (
        <div className="flex flex-col gap-2 text-[13px]">
          <p className="m-0 flex items-center gap-2 text-fg-2">
            <CircleCheck size={15} strokeWidth={1.75} aria-hidden className="text-success" />
            {t("leftover.removed", { n: report.removed })}
          </p>
          {report.failed.length > 0 && (
            <>
              <p className="m-0 text-[12px] text-warning">
                {t(report.needsAdmin ? "leftover.failedAdmin" : "leftover.failed", { n: report.failed.length })}
              </p>
              <div className="max-h-[30vh] overflow-y-auto rounded-lg border border-line">
                {report.failed.map((failure, i) => (
                  <div key={i} className="flex items-center gap-3 border-b border-line px-3 py-1.5 last:border-b-0">
                    <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px]" title={label(failure.item)}>
                      {label(failure.item)}
                    </span>
                    <span className="shrink-0 text-[12px] text-fg-3">{failureText(t, failure.reason)}</span>
                  </div>
                ))}
              </div>
            </>
          )}
          {report.backup && (
            <p className="m-0 flex items-center gap-2 text-[12px] text-fg-3">
              <span className="min-w-0 flex-1 truncate" title={report.backup}>
                {t("leftover.backup", { path: report.backup })}
              </span>
              <Button variant="ghost" size="sm" onClick={() => void api.openUninstallBackup(report.backup!)}>
                <FolderOpen size={13} strokeWidth={1.75} aria-hidden />
                {t("leftover.openBackup")}
              </Button>
            </p>
          )}
        </div>
      )}
    </Modal>
  );
}

function Busy({ text, hint }: { text: string; hint?: string }) {
  return (
    <div className="py-2">
      <div className="flex items-center gap-2 text-[13px] text-fg-2">
        <LoaderCircle size={15} strokeWidth={2} className="animate-spin" aria-hidden />
        {text}
      </div>
      {hint && <p className="m-0 mt-2 ml-6 text-[12px] leading-relaxed text-fg-3">{hint}</p>}
    </div>
  );
}

/** 一项残留在列表里显示的样子：路径；注册表值写成“键 → 值名”。 */
function label(item: Leftover): string {
  return item.value ? `${item.path} → ${item.value}` : item.path;
}

function failureText(t: ReturnType<typeof useT>, reason: string): string {
  const key = `leftover.fail.${reason}` as Key;
  const text = t(key);
  return text === key ? reason : text;
}

/** 按“文件和文件夹”“注册表”分两组列出来，每行一个勾选框。 */
function LeftoverList({ items, checked, onToggle }: { items: Leftover[]; checked: Set<number>; onToggle: (index: number, on: boolean) => void }) {
  const t = useT();
  const groups = [
    { title: t("leftover.groupFiles"), entries: items.map((item, i) => ({ item, i })).filter(({ item }) => item.kind === "folder" || item.kind === "file") },
    { title: t("leftover.groupRegistry"), entries: items.map((item, i) => ({ item, i })).filter(({ item }) => item.kind === "registryKey" || item.kind === "registryValue") },
  ].filter((group) => group.entries.length > 0);
  return (
    <div className="max-h-[46vh] overflow-y-auto rounded-lg border border-line">
      {groups.map((group) => (
        <section key={group.title}>
          <div className="sticky top-0 z-10 border-b border-line bg-panel px-3 py-1.5 text-[12px] font-medium text-fg-3">
            {group.title} <span className="tabular text-fg-4">{group.entries.length}</span>
          </div>
          {group.entries.map(({ item, i }) => {
            const Icon = item.kind === "folder" ? Folder : item.kind === "file" ? File : Database;
            const on = checked.has(i);
            return (
              <div key={i} className={cx("flex items-center gap-2.5 border-b border-line px-3 py-1.5 last:border-b-0", !on && "opacity-70")}>
                <Checkbox checked={on} onChange={(value) => onToggle(i, value)}>
                  <span className="sr-only">{label(item)}</span>
                </Checkbox>
                <Icon size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-3" />
                <span className="selectable min-w-0 flex-1 truncate font-mono text-[12px]" title={`${label(item)}\n${t(`leftover.reason.${item.reason}` as Key)}`}>
                  {label(item)}
                </span>
                {item.size !== null && item.size > 0 && <span className="tabular shrink-0 text-[12px] text-fg-3">{formatSize(item.size)}</span>}
                <span
                  title={t(`leftover.reason.${item.reason}` as Key)}
                  className={cx(
                    "shrink-0 rounded px-1.5 text-[12px]",
                    item.confidence === "high" ? "bg-accent-soft text-accent" : item.confidence === "medium" ? "bg-hover text-fg-2" : "text-fg-3",
                  )}
                >
                  {t(`leftover.confidence.${item.confidence}` as Key)}
                </span>
              </div>
            );
          })}
        </section>
      ))}
    </div>
  );
}
