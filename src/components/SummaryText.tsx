// 详情栏里的软件简介：软件库的软件和只装在本机的软件共用。

import { LoaderCircle, RefreshCw } from "lucide-react";
import { useState } from "react";
import { shownSummary } from "../lib/library";
import { useApp, useLanguage, useT } from "../lib/store";
import type { Summary } from "../lib/types";

/**
 * 简介。开着自动翻译、又有译文时显示译文，下面一行小字说明是机器翻译的，点一下看原文。
 * 给了 `onFetch` 时：正在取时显示“正在获取简介”，取过但没找到、或者还没取过时给一个“获取简介”的按钮。
 */
export function SummaryText({ summary, busy = false, onFetch }: { summary: Summary | undefined; busy?: boolean; onFetch?: () => void }) {
  const t = useT();
  const language = useLanguage();
  const translate = useApp((state) => state.library?.settings.translateSummaries ?? true);
  const [original, setOriginal] = useState(false);
  const shown = shownSummary(summary, language, translate && !original);

  if (busy && !shown) {
    return (
      <div className="mt-3 flex items-center gap-1.5 text-[12px] text-fg-3">
        <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
        {t("summary.loading")}
      </div>
    );
  }
  if (!summary || !shown) {
    if (!onFetch) return null;
    // 取过但没找到 / 还没取过（比如关着“联网获取简介”）
    return (
      <div className="mt-3 flex items-center gap-2 text-[12px] text-fg-3">
        <span>{summary ? t("summary.none") : t("summary.notYet")}</span>
        <button type="button" onClick={onFetch} className="flex items-center gap-1 rounded px-1 text-accent hover:bg-hover">
          <RefreshCw size={12} strokeWidth={2} aria-hidden />
          {summary ? t("summary.retry") : t("summary.fetch")}
        </button>
      </div>
    );
  }
  const hasTranslation = translate && Boolean(summary.translations?.[language]);
  return (
    <div className="mt-3">
      <p
        className="selectable m-0 line-clamp-4 text-[13px] leading-relaxed text-fg-2"
        title={`${shown.text}\n\n${t("summary.source", { source: summary.source })}`}
      >
        {shown.text}
      </p>
      {hasTranslation && (
        <button type="button" onClick={() => setOriginal(!original)} className="mt-1 text-[12px] text-fg-3 hover:text-fg-2">
          {original ? t("summary.showTranslation") : t("summary.translated")}
        </button>
      )}
    </div>
  );
}
