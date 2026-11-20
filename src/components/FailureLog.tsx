// 设置里的“失败日志”：导入、更新、启动项、AI 这些操作失败时记下的原因，新的在前。
// 日志存在应用自己的日志文件夹里（只和这台电脑有关），可以打开文件夹、清空。

import { FolderOpen, LoaderCircle, RefreshCw, ScrollText, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Key } from "../lib/i18n";
import { useApp, useLanguage, useT } from "../lib/store";
import type { FailureEntry } from "../lib/types";
import { confirm } from "./dialogs";
import { Button, EmptyState, IconButton } from "./ui";

export function FailureLogPage() {
  const t = useT();
  const language = useLanguage();
  const attempt = useApp((state) => state.attempt);
  const [entries, setEntries] = useState<FailureEntry[] | null>(null);

  const load = async () => {
    setEntries(null);
    setEntries((await attempt(() => api.readFailureLog(500))) ?? []);
  };
  useEffect(() => {
    void load();
    // 打开这一页时读一次
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const clear = async () => {
    const { ok } = await confirm({ title: t("log.clearTitle"), body: t("log.clearBody"), confirmLabel: t("log.clear"), danger: true });
    if (!ok) return;
    await attempt(() => api.clearFailureLog());
    setEntries([]);
  };

  const area = (name: string) => {
    const key = `log.area.${name}` as Key;
    const text = t(key);
    return text === key ? name : text;
  };
  const time = (iso: string) => {
    const date = new Date(iso);
    return Number.isNaN(date.getTime()) ? iso : date.toLocaleString(language, { dateStyle: "short", timeStyle: "medium" });
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2">
        <p className="m-0 min-w-0 flex-1 text-[12px] leading-relaxed text-fg-3">{t("log.hint")}</p>
        <IconButton label={t("log.reload")} onClick={() => void load()}>
          <RefreshCw size={14} strokeWidth={1.75} />
        </IconButton>
        <Button size="sm" onClick={() => void attempt(() => api.openLogFolder())}>
          <FolderOpen size={13} strokeWidth={1.75} aria-hidden />
          {t("log.open")}
        </Button>
        <Button size="sm" variant="caution" disabled={!entries?.length} onClick={() => void clear()}>
          <Trash2 size={13} strokeWidth={1.75} aria-hidden />
          {t("log.clear")}
        </Button>
      </div>
      <div className="max-h-[52vh] min-h-[200px] overflow-y-auto rounded-lg border border-line">
        {entries === null ? (
          <div className="flex items-center gap-2 px-3 py-3 text-[12px] text-fg-3">
            <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
            {t("log.loading")}
          </div>
        ) : entries.length === 0 ? (
          <div className="py-8">
            <EmptyState icon={ScrollText} title={t("log.empty")} />
          </div>
        ) : (
          entries.map((entry, index) => (
            <div key={`${entry.time}-${index}`} className="border-b border-line px-3 py-2 last:border-b-0">
              <div className="flex items-center gap-2 text-[12px] text-fg-3">
                <span className="tabular shrink-0">{time(entry.time)}</span>
                <span className="shrink-0 rounded-[4px] bg-hover px-1.5">{area(entry.area)}</span>
              </div>
              <div className="selectable mt-0.5 text-[13px] leading-relaxed break-words text-danger">{entry.message}</div>
              {entry.detail && <div className="selectable mt-0.5 font-mono text-[12px] break-all text-fg-3">{entry.detail}</div>}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
