// 智能分类的预览窗口：列出每个软件的建议，可以逐个取消或改成别的分类，确认后才真正归类。

import { ArrowRight, LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { categoryPath, flattenTree } from "../lib/library";
import { useApp, useT } from "../lib/store";
import type { Assignment, Software, Suggestion } from "../lib/types";
import { SoftwareIcon, useSoftwareIcon } from "./SoftwareIcon";
import { Button, Checkbox, Modal, Select, cx } from "./ui";

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

  const [suggestions, setSuggestions] = useState<Suggestion[] | null>(null);
  const [choices, setChoices] = useState<Record<string, Choice>>({});
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    api.suggestCategories(ids).then(
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
      () => alive && setSuggestions([]),
    );
    return () => {
      alive = false;
    };
  }, [ids]);

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
      {suggestions === null ? (
        <div className="flex items-center gap-2 py-6 text-[13px] text-fg-3">
          <LoaderCircle size={15} strokeWidth={2} className="animate-spin" aria-hidden />
          {t("classify.loading")}
        </div>
      ) : (
        <>
          <p className="m-0 mb-3 text-[13px] leading-relaxed text-fg-2">
            {found > 0 ? t("classify.summary", { n: found }) : t("classify.none")}
          </p>
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
  const icon = useSoftwareIcon(software);
  const reason = suggestion.unchanged
    ? t("classify.unchanged")
    : suggestion.known
      ? t("classify.known")
      : suggestion.evidence.length > 0
        ? t("classify.evidence", { terms: suggestion.evidence.slice(0, 4).join("、") })
        : !suggestion.categoryId && !suggestion.newCategory
          ? t("classify.unknown")
          : "";
  return (
    <div className={cx("flex items-center gap-3 border-b border-line px-3 py-2 last:border-b-0", !checked && "opacity-60")}>
      <Checkbox checked={checked} onChange={onCheck}>
        <span className="sr-only">{software.name}</span>
      </Checkbox>
      <SoftwareIcon name={software.name} icon={icon} size={28} />
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
