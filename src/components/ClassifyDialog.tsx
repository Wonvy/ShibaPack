// 智能分类的预览窗口：列出每个软件的建议，可以逐个取消或改成别的分类，确认后才真正归类。
// 设置好了 AI 时默认让 AI 判断，也可以换回内置规则（不联网，按常见软件名单和关键词）。

import { ArrowRight, LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, onAiProgress } from "../lib/api";
import { categoryPath, flattenTree } from "../lib/library";
import { errorText } from "../lib/i18n";
import { useAiReady, useApp, useLanguage, useT } from "../lib/store";
import type { AiProgress, Assignment, Software, Suggestion } from "../lib/types";
import { RepositorySoftwareIcon } from "./SoftwareIcon";
import { Button, Checkbox, Modal, Segmented, Select, cx } from "./ui";

type Mode = "ai" | "rules";

/** 每一行的选择：`""` 不改；`cat:<id>` 放进已有分类；`new:<名字>` 新建分类。 */
type Choice = string;

export function ClassifyDialog() {
  const ids = useApp((state) => state.classifyIds);
  if (!ids) return null;
  return <Dialog key={ids.join(",")} ids={ids} />;
}

function Dialog({ ids }: { ids: string[] }) {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const patch = useApp((state) => state.patch);
  const apply = useApp((state) => state.apply);
  const notify = useApp((state) => state.notify);
  const close = () => patch({ classifyIds: null });

  const language = useLanguage();
  const aiReady = useAiReady();
  const [mode, setMode] = useState<Mode>(aiReady ? "ai" : "rules");
  const [aiProgress, setAiProgress] = useState<AiProgress | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[] | null>(null);
  /** 取建议失败（比如仓库打不开）：说出原因，不要显示成“认不出这些软件”。 */
  const [failure, setFailure] = useState<string | null>(null);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);
  /** 这次有几个用的是上次 AI 的建议。 */
  const [cachedCount, setCachedCount] = useState(0);
  /** 点了“重新分析”：加一，重新问 AI（不用缓存）。 */
  const [rerun, setRerun] = useState(0);

  useEffect(() => {
    let alive = true;
    setSuggestions(null);
    setFailure(null);
    setAiProgress(null);
    const useAi = mode === "ai";
    let stopListening: (() => void) | null = null;
    if (useAi) void onAiProgress((progress) => alive && setAiProgress(progress)).then((stop) => (alive ? (stopListening = stop) : stop()));
    setCachedCount(0);
    // 没变过的软件直接用上次 AI 的建议，不再问；点了“重新分析”才全部重新问
    const request = useAi
      ? api.aiSuggestCategories(ids, rerun > 0).then((result) => {
          if (alive) setCachedCount(result.cached);
          return result.suggestions;
        })
      : api.suggestCategories(ids);
    request.then(
      (list) => {
        if (!alive) return;
        setSuggestions(list);
        const nextChoices: Record<string, Choice> = {};
        const nextChecked: Record<string, boolean> = {};
        for (const s of list) {
          nextChoices[s.softwareId] = s.categoryId ? `cat:${s.categoryId}` : s.newCategory ? `new:${s.newCategory.name}` : "";
          // 有建议、而且确实要挪动的才默认勾上
          nextChecked[s.softwareId] = Boolean((s.categoryId || s.newCategory) && !s.unchanged);
        }
        setChoices(nextChoices);
        setChecked(nextChecked);
      },
      (error) => {
        if (!alive) return;
        setFailure(errorText(language, error));
        setSuggestions([]);
      },
    );
    return () => {
      alive = false;
      stopListening?.();
      // 关掉窗口或换回内置规则时，后面几批不用再问 AI 了
      if (useAi) void api.cancelAi();
    };
  }, [ids, mode, rerun]);

  // 下拉框里可选的新建分类：所有建议里提到过的（一批里可能有好几个）
  const newNames = useMemo(
    () => [...new Set((suggestions ?? []).flatMap((s) => (s.newCategory ? [s.newCategory.name] : [])))],
    [suggestions],
  );
  const newIcons = useMemo(() => {
    const map: Record<string, string> = {};
    for (const s of suggestions ?? []) if (s.newCategory) map[s.newCategory.name] = s.newCategory.icon;
    return map;
  }, [suggestions]);
  const tree = flattenTree(library.categories);
  const softwareOf = (id: string) => library.software.find((s) => s.id === id);

  const pending = (suggestions ?? []).filter((s) => {
    const choice = choices[s.softwareId];
    const current = softwareOf(s.softwareId)?.categoryId ?? null;
    return checked[s.softwareId] && choice && choice !== `cat:${current}`;
  });
  const found = (suggestions ?? []).filter((s) => s.categoryId || s.newCategory).length;

  const confirmAll = async () => {
    const assignments: Assignment[] = pending.map((s) => {
      const choice = choices[s.softwareId];
      if (choice.startsWith("cat:")) return { softwareId: s.softwareId, categoryId: choice.slice(4) };
      const name = choice.slice(4);
      return { softwareId: s.softwareId, newCategory: { name, icon: newIcons[name] ?? "folder" } };
    });
    setBusy(true);
    const ok = await apply(() => api.applyClassification(assignments));
    setBusy(false);
    if (!ok) return;
    notify(t("classify.done", { n: assignments.length }));
    close();
  };

  return (
    <Modal
      open
      onClose={close}
      title={t("classify.title")}
      width={680}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy} disabled={pending.length === 0} onClick={() => void confirmAll()}>
            {t("classify.confirm", { n: pending.length })}
          </Button>
        </>
      }
    >
      <div className="mb-3 flex items-start gap-3">
        {suggestions === null ? (
          <div className="flex min-h-7 flex-1 items-center gap-2 text-[13px] text-fg-3">
            <LoaderCircle size={15} strokeWidth={2} className="animate-spin" aria-hidden />
            {mode === "ai" ? t("classify.aiLoading", { done: aiProgress?.done ?? 0, total: aiProgress?.total ?? ids.length }) : t("classify.loading")}
          </div>
        ) : (
          <div className="min-h-7 flex-1 pt-1">
            <p className={cx("m-0 text-[13px] leading-relaxed", failure ? "text-danger" : "text-fg-2")}>
              {failure ?? (found > 0 ? t("classify.summary", { n: found }) : t("classify.none"))}
            </p>
            {mode === "ai" && cachedCount > 0 && !failure && (
              <p className="m-0 mt-0.5 flex flex-wrap items-center gap-x-2 text-[12px] text-fg-3">
                {t("classify.cached", { n: cachedCount })}
                <button type="button" className="font-medium text-accent hover:underline" onClick={() => setRerun(rerun + 1)}>
                  {t("classify.rerun")}
                </button>
              </p>
            )}
          </div>
        )}
        {aiReady && (
          <Segmented<Mode>
            value={mode}
            onChange={setMode}
            options={[
              { value: "ai", label: t("classify.byAi") },
              { value: "rules", label: t("classify.byRules") },
            ]}
          />
        )}
      </div>
      {suggestions !== null && (
        <>
          <div className="max-h-[56vh] overflow-y-auto rounded-lg border border-line">
            {suggestions.map((suggestion) => {
              const software = softwareOf(suggestion.softwareId);
              if (!software) return null;
              return (
                <Row
                  key={suggestion.softwareId}
                  software={software}
                  suggestion={suggestion}
                  currentName={software.categoryId ? categoryPath(library.categories, software.categoryId) : t("nav.uncategorized")}
                  choice={choices[suggestion.softwareId] ?? ""}
                  checked={checked[suggestion.softwareId] ?? false}
                  onCheck={(value) => setChecked({ ...checked, [suggestion.softwareId]: value })}
                  onChoose={(value) => {
                    setChoices({ ...choices, [suggestion.softwareId]: value });
                    // 手动选了一个分类，就当作要分它
                    setChecked({ ...checked, [suggestion.softwareId]: value !== "" });
                  }}
                  options={
                    <>
                      <option value="">{t("classify.keep")}</option>
                      {newNames.map((name) => (
                        <option key={`new:${name}`} value={`new:${name}`}>
                          {t("classify.new", { name })}
                        </option>
                      ))}
                      {tree.map(({ category, depth }) => (
                        <option key={category.id} value={`cat:${category.id}`}>
                          {"　".repeat(depth) + category.name}
                        </option>
                      ))}
                    </>
                  }
                />
              );
            })}
          </div>
        </>
      )}
    </Modal>
  );
}

function Row({
  software,
  suggestion,
  currentName,
  choice,
  checked,
  onCheck,
  onChoose,
  options,
}: {
  software: Software;
  suggestion: Suggestion;
  currentName: string;
  choice: Choice;
  checked: boolean;
  onCheck: (value: boolean) => void;
  onChoose: (value: Choice) => void;
  options: React.ReactNode;
}) {
  const t = useT();
  const reason = suggestion.unchanged
    ? t("classify.unchanged")
    : suggestion.reason
      ? t("classify.aiReason", { reason: suggestion.reason })
      : suggestion.known
      ? t("classify.known")
      : suggestion.evidence.length > 0
        ? t("classify.evidence", { terms: suggestion.evidence.slice(0, 4).join(t("common.listSeparator")) })
        : !suggestion.categoryId && !suggestion.newCategory
          ? t("classify.unknown")
          : "";
  return (
    <div className={cx("flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0", !checked && "opacity-60")}>
      <Checkbox checked={checked} onChange={onCheck}>
        <span className="sr-only">{software.name}</span>
      </Checkbox>
      <RepositorySoftwareIcon software={software} size={28} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-[13px] font-medium">{software.name}</div>
        <div className="truncate text-[12px] text-fg-3" title={reason}>
          {currentName}
          {reason && <span className="text-fg-4"> · {reason}</span>}
        </div>
      </div>
      <ArrowRight size={14} strokeWidth={1.75} aria-hidden className="shrink-0 text-fg-4" />
      <div className="w-[200px] shrink-0">
        <Select aria-label={software.name} value={choice} onChange={(event) => onChoose(event.target.value)}>
          {options}
        </Select>
      </div>
    </div>
  );
}
