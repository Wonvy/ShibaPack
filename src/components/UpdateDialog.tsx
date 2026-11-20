// “软件更新”窗口：点底栏右下角的版本号打开。
// 显示当前版本、有没有新版本，以及新版本的更新说明；有新版本时可以直接更新。
// 更新说明优先用更新包里带的（latest.json 的 notes），没有时去 GitHub 上取那个版本的 Release 说明。

import { AlertTriangle, CircleCheck, ExternalLink, LoaderCircle, Sparkles } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { api } from "../lib/api";
import { useApp, useLanguage, useT } from "../lib/store";
import { checkForUpdate, finishUpdate, startUpdate } from "../lib/updater";
import { Logo } from "./chrome";
import { Button, Modal, cx } from "./ui";

const RELEASES = "https://github.com/Wonvy/ShibaPack/releases/tag/v";

/** 这次运行里取过的更新说明，按版本号。 */
const fetched = new Map<string, string>();

/**
 * 发布说明 → 这次的更新内容：去掉下载说明（单独一行 `---` 之后的部分）和开头的版本号标题；
 * 有“### 中文”“### English”两段时按界面语言挑一段。
 */
export function releaseNotesFor(text: string, language: string): string {
  let body = text.replace(/\r/g, "");
  const cut = body.search(/^---\s*$/m);
  if (cut >= 0) body = body.slice(0, cut);
  body = body.replace(/^##\s+v?\d[^\n]*\n/, "").trim();
  const sections = body.split(/^###\s+/m).filter((part) => part.trim());
  if (sections.length > 1) {
    const zh = language.toLowerCase().startsWith("zh");
    const pick = sections.find((part) => (zh ? /^(中文|简体中文)/.test(part) : /^English/i.test(part)));
    if (pick) return pick.replace(/^[^\n]*\n/, "").trim();
  }
  return body;
}

/** 把一点点 Markdown 画出来：列表、小标题、段落；链接只留文字。 */
function Notes({ text }: { text: string }) {
  const lines = text.split("\n");
  const blocks: ReactNode[] = [];
  let list: string[] = [];
  const plain = (line: string) => line.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1").replace(/\*\*([^*]+)\*\*/g, "$1").replace(/`([^`]+)`/g, "$1");
  const flush = () => {
    if (list.length === 0) return;
    blocks.push(
      <ul key={`l${blocks.length}`} className="m-0 flex list-none flex-col gap-1 p-0">
        {list.map((item, i) => (
          <li key={i} className="flex gap-2">
            <span aria-hidden className="mt-[7px] size-1 shrink-0 rounded-full bg-fg-3" />
            <span className="min-w-0">{plain(item)}</span>
          </li>
        ))}
      </ul>,
    );
    list = [];
  };
  for (const raw of lines) {
    const line = raw.trim();
    if (/^[-*]\s+/.test(line)) {
      list.push(line.replace(/^[-*]\s+/, ""));
      continue;
    }
    flush();
    if (!line) continue;
    if (/^#{1,6}\s+/.test(line)) {
      blocks.push(
        <div key={`h${blocks.length}`} className="mt-1 font-medium text-fg first:mt-0">
          {plain(line.replace(/^#{1,6}\s+/, ""))}
        </div>,
      );
    } else {
      blocks.push(
        <p key={`p${blocks.length}`} className="m-0">
          {plain(line)}
        </p>,
      );
    }
  }
  flush();
  return <div className="selectable flex flex-col gap-2 text-[13px] leading-relaxed text-fg-2">{blocks}</div>;
}

export function UpdateDialog() {
  const t = useT();
  const language = useLanguage();
  const open = useApp((state) => state.updateDialog);
  const patch = useApp((state) => state.patch);
  const attempt = useApp((state) => state.attempt);
  const current = useApp((state) => state.status?.version ?? "");
  const update = useApp((state) => state.appUpdate);
  const newer = ["available", "downloading", "ready", "installed"].includes(update.phase);
  // 有新版本时看新版本的说明，否则看当前版本的说明
  const version = newer ? update.version : current;
  const [notes, setNotes] = useState<{ version: string; text: string | null; failed: boolean } | null>(null);

  useEffect(() => {
    if (!open || !version) return;
    const inline = newer ? update.notes : "";
    if (inline) {
      setNotes({ version, text: inline, failed: false });
      return;
    }
    const known = fetched.get(version);
    if (known !== undefined) {
      setNotes({ version, text: known, failed: false });
      return;
    }
    let alive = true;
    setNotes({ version, text: null, failed: false });
    api.releaseNotes(version).then(
      (text) => {
        fetched.set(version, text);
        if (alive) setNotes({ version, text, failed: false });
      },
      () => alive && setNotes({ version, text: "", failed: true }),
    );
    return () => {
      alive = false;
    };
  }, [open, version, newer, update.notes]);

  if (!open) return null;
  const close = () => patch({ updateDialog: false });
  const percent = update.progress === null ? null : Math.round(update.progress * 100);
  const shown = notes && notes.version === version && notes.text !== null ? releaseNotesFor(notes.text, language) : null;

  // 右下角的主按钮跟着进度变：有新版本时“立即更新”，下载好了“安装并重启”或“立即重启”，出错了“重试”
  const primary =
    update.phase === "available" ? (
      <Button variant="primary" onClick={() => void startUpdate()}>
        {t("update.install")}
      </Button>
    ) : update.phase === "ready" ? (
      <Button variant="primary" onClick={() => void finishUpdate()}>
        {t("update.installRestart")}
      </Button>
    ) : update.phase === "installed" ? (
      <Button variant="primary" onClick={() => void finishUpdate()}>
        {t("update.restart")}
      </Button>
    ) : update.phase === "downloading" ? (
      <Button variant="primary" busy>
        {t("update.downloadingShort")}
      </Button>
    ) : (
      <Button busy={update.phase === "checking"} onClick={() => void checkForUpdate(true)}>
        {update.phase === "error" ? t("update.retry") : t("update.check")}
      </Button>
    );

  return (
    <Modal
      open
      onClose={close}
      title={t("update.title")}
      width={460}
      footer={
        <>
          <Button variant="ghost" className="mr-auto" onClick={() => void attempt(() => api.openUrl(`${RELEASES}${version}`))}>
            <ExternalLink size={13} strokeWidth={1.75} aria-hidden />
            {t("update.onGithub")}
          </Button>
          <Button variant="ghost" onClick={close}>
            {newer && update.phase === "available" ? t("update.later") : t("common.close")}
          </Button>
          {primary}
        </>
      }
    >
      <div className="flex items-center gap-3">
        <Logo size={40} />
        <div className="min-w-0 flex-1">
          <div className="text-[15px] font-semibold">ShibaPack</div>
          <div className="tabular text-[12px] text-fg-3">{t("update.current", { version: current })}</div>
        </div>
      </div>

      {/* 现在的状态：一行，带图标 */}
      <div
        className={cx(
          "mt-4 flex items-center gap-2 rounded-lg px-3 py-2.5 text-[13px]",
          newer ? "bg-accent-soft text-accent" : update.phase === "error" ? "bg-hover text-danger" : "bg-hover text-fg-2",
        )}
      >
        {update.phase === "checking" ? (
          <LoaderCircle size={15} strokeWidth={2} className="shrink-0 animate-spin" aria-hidden />
        ) : newer ? (
          <Sparkles size={15} strokeWidth={1.75} className="shrink-0" aria-hidden />
        ) : update.phase === "error" ? (
          <AlertTriangle size={15} strokeWidth={1.75} className="shrink-0" aria-hidden />
        ) : (
          <CircleCheck size={15} strokeWidth={1.75} className="shrink-0 text-success" aria-hidden />
        )}
        <span className="min-w-0 flex-1 break-words">
          {update.phase === "checking" && t("update.checking")}
          {update.phase === "upToDate" && t("update.upToDate")}
          {update.phase === "idle" && t("update.upToDate")}
          {update.phase === "error" && t("update.failed", { message: update.error })}
          {update.phase === "available" && t("update.available", { version: update.version })}
          {update.phase === "downloading" && t("update.downloading", { version: update.version })}
          {update.phase === "ready" && t("update.ready", { version: update.version })}
          {update.phase === "installed" && t("update.installed", { version: update.version })}
        </span>
        {update.phase === "downloading" && percent !== null && <span className="tabular shrink-0 text-[12px]">{percent}%</span>}
      </div>
      {update.phase === "downloading" && (
        <div className="mt-2 h-1 overflow-hidden rounded-full bg-line">
          {percent === null ? (
            <div className="h-full w-2/5 animate-slide bg-accent" />
          ) : (
            <div className="h-full bg-accent transition-[width] duration-200" style={{ width: `${percent}%` }} />
          )}
        </div>
      )}

      {/* 更新说明 */}
      <h3 className="m-0 mt-5 mb-2 text-[12px] font-medium text-fg-3">
        {newer ? t("update.whatsNew", { version }) : t("update.thisVersion", { version })}
      </h3>
      <div className="max-h-[260px] min-h-16 overflow-y-auto rounded-lg border border-line px-3.5 py-3">
        {notes === null || notes.version !== version || notes.text === null ? (
          <div className="flex items-center gap-2 text-[12px] text-fg-3">
            <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
            {t("update.loadingNotes")}
          </div>
        ) : shown ? (
          <Notes text={shown} />
        ) : (
          <div className="text-[12px] text-fg-3">{notes.failed ? t("update.notesFailed") : t("update.noNotes")}</div>
        )}
      </div>
    </Modal>
  );
}
