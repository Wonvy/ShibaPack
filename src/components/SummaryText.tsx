// 详情栏里的软件简介：软件库的软件和只装在本机的软件共用。

import { LoaderCircle, RefreshCw, WandSparkles } from "lucide-react";
import { useState } from "react";
import { shownSummary } from "../lib/library";
import { useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { Summary } from "../lib/types";

/** 简介按什么名字存，和发布者（只装在本机的软件用）。给了才出现“AI 写简介”。 */
export interface AiTarget {
  name: string;
  publisher?: string;
}

const linkClass = "flex items-center gap-1 rounded px-1 text-accent hover:bg-hover";

/**
 * 简介。开着自动翻译、又有译文时显示译文，下面一行小字说明是机器翻译的，点一下看原文。
 * 给了 `onFetch` 时：正在取时显示“正在获取简介”，取过但没找到、或者还没取过时给一个“获取简介”的按钮。
 * 给了 `ai`、又设置好了 AI 时：没有简介可以让 AI 写，有简介可以让 AI 重写。AI 写的简介下面注明“由 AI 生成”。
 */
export function SummaryText({
  summary,
  busy = false,
  onFetch,
  ai,
}: {
  summary: Summary | undefined;
  busy?: boolean;
  onFetch?: () => void;
  ai?: AiTarget;
}) {
  const t = useT();
  const language = useLanguage();
  const translate = useApp((state) => state.library?.settings.translateSummaries ?? true);
  const aiReady = useAiReady();
  const writing = useApp((state) => (ai ? Boolean(state.aiBusy[ai.name]) : false));
  const describeWithAi = useApp((state) => state.describeWithAi);
  const [original, setOriginal] = useState(false);
  const shown = shownSummary(summary, language, translate && !original);
  const canWrite = Boolean(ai && aiReady);
  const write = () => ai && void describeWithAi([ai]);

  if (writing) {
    return (
      <div className="mt-3 flex items-center gap-1.5 text-[12px] text-fg-3">
        <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
        {t("ai.writing")}
      </div>
    );
  }
  if (busy && !shown) {
    return (
      <div className="mt-3 flex items-center gap-1.5 text-[12px] text-fg-3">
        <LoaderCircle size={13} strokeWidth={2} className="animate-spin" aria-hidden />
        {t("summary.loading")}
      </div>
    );
  }
  if (!summary || !shown) {
    if (!onFetch && !canWrite) return null;
    // 取过但没找到 / 还没取过（比如关着“联网获取简介”）
    return (
      <div className="mt-3 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-fg-3">
        <span>{summary ? t("summary.none") : t("summary.notYet")}</span>
        {onFetch && (
          <button type="button" onClick={onFetch} className={linkClass}>
            <RefreshCw size={12} strokeWidth={2} aria-hidden />
            {summary ? t("summary.retry") : t("summary.fetch")}
          </button>
        )}
        {canWrite && (
          <button type="button" onClick={write} className={linkClass}>
            <WandSparkles size={12} strokeWidth={2} aria-hidden />
            {t("ai.describe")}
          </button>
        )}
      </div>
    );
  }
  const hasTranslation = translate && Boolean(summary.translations?.[language]);
  const byAi = summary.source.startsWith("AI");
  return (
    <div className="group/summary mt-3">
      <p
        className="selectable m-0 line-clamp-4 text-[13px] leading-relaxed text-fg-2"
        title={`${shown.text}\n\n${t("summary.source", { source: summary.source })}`}
      >
        {shown.text}
      </p>
      {(hasTranslation || byAi || canWrite) && (
        <div className="mt-1 flex items-center gap-2 text-[12px] text-fg-3">
          {hasTranslation && (
            <button type="button" onClick={() => setOriginal(!original)} className="hover:text-fg-2">
              {original ? t("summary.showTranslation") : t("summary.translated")}
            </button>
          )}
          {byAi && !hasTranslation && <span>{t("ai.written")}</span>}
          {canWrite && (
            // 平常不占眼，鼠标停在简介上或用键盘移过来时才显示
            <button
              type="button"
              onClick={write}
              className="ml-auto flex items-center gap-1 rounded px-1 opacity-0 group-hover/summary:opacity-100 hover:bg-hover hover:text-fg-2 focus-visible:opacity-100"
            >
              <WandSparkles size={12} strokeWidth={2} aria-hidden />
              {t("ai.rewrite")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
