// 标签管理：新建、改名、改颜色、删除，并显示每个标签用在多少软件上。

import { Pencil, Plus, Tags, Trash2 } from "lucide-react";
import { useState } from "react";
import { confirm } from "../components/dialogs";
import { Button, EmptyState, Field, IconButton, Modal, TextInput, cx } from "../components/ui";
import { api } from "../lib/api";
import { CATEGORY_COLORS } from "../lib/icons";
import { newId } from "../lib/library";
import { useApp, useT } from "../lib/store";
import type { Tag } from "../lib/types";

export function TagsView() {
  const t = useT();
  const library = useApp((state) => state.library)!;
  const apply = useApp((state) => state.apply);
  const setRoute = useApp((state) => state.setRoute);
  const patch = useApp((state) => state.patch);
  const [editing, setEditing] = useState<Tag | "new" | null>(null);

  const usage = (id: string) => library.software.filter((s) => s.tagIds.includes(id)).length;

  const remove = async (tag: Tag) => {
    const { ok } = await confirm({
      title: t("common.delete"),
      body: t("tags.deleteConfirm", { name: tag.name, n: usage(tag.id) }),
      confirmLabel: t("common.delete"),
      danger: true,
    });
    if (ok) await apply(() => api.setTags(library.tags.filter((x) => x.id !== tag.id)));
  };

  // 点一个标签：回到软件库并按它筛选
  const show = (tag: Tag) => {
    patch({ tagFilter: tag.id });
    setRoute("library");
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center justify-between border-b border-line pr-2 pl-4">
        <h2 className="m-0 text-[13px] font-semibold">{t("tags.title")}</h2>
        <Button onClick={() => setEditing("new")}>
          <Plus size={14} strokeWidth={2} aria-hidden />
          {t("tags.new")}
        </Button>
      </div>
      {library.tags.length === 0 ? (
        <EmptyState icon={Tags} title={t("tags.empty")} body={t("tags.emptyBody")} />
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[640px] px-4 py-3">
            {library.tags.map((tag) => (
              <div key={tag.id} className="group flex h-10 items-center gap-3 border-b border-line">
                <span className="size-2.5 shrink-0 rounded-full" style={{ background: tag.color ?? "var(--fg-3)" }} />
                <button type="button" onClick={() => show(tag)} className="min-w-0 flex-1 truncate text-left text-[13px] hover:text-accent">
                  {tag.name}
                </button>
                <span className="tabular text-[12px] text-fg-3">{t("tags.usage", { n: usage(tag.id) })}</span>
                <div className="flex opacity-0 transition-opacity group-focus-within:opacity-100 group-hover:opacity-100">
                  <IconButton label={t("common.edit")} onClick={() => setEditing(tag)}>
                    <Pencil size={14} strokeWidth={1.75} />
                  </IconButton>
                  <IconButton label={t("common.delete")} onClick={() => void remove(tag)}>
                    <Trash2 size={14} strokeWidth={1.75} />
                  </IconButton>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
      {editing && <TagDialog tag={editing === "new" ? null : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

function TagDialog({ tag, onClose }: { tag: Tag | null; onClose: () => void }) {
  const t = useT();
  const tags = useApp((state) => state.library!.tags);
  const apply = useApp((state) => state.apply);
  const [name, setName] = useState(tag?.name ?? "");
  const [color, setColor] = useState<string | null>(tag?.color ?? CATEGORY_COLORS[0]);

  const save = async () => {
    if (!name.trim()) return;
    const next: Tag = { id: tag?.id ?? newId(), name: name.trim(), color };
    const list = tag ? tags.map((x) => (x.id === tag.id ? next : x)) : [...tags, next];
    if (await apply(() => api.setTags(list))) onClose();
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={tag ? t("common.edit") : t("tags.new")}
      width={340}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!name.trim()} onClick={() => void save()}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5">
        <Field label={t("tags.name")}>
          <TextInput autoFocus value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void save()} />
        </Field>
        <div>
          <span className="mb-1.5 block text-[12px] font-medium text-fg-2">{t("cat.color")}</span>
          <div className="flex items-center gap-1.5">
            {CATEGORY_COLORS.map((value) => (
              <button
                key={value}
                type="button"
                aria-label={value}
                aria-pressed={color === value}
                onClick={() => setColor(value)}
                style={{ background: value }}
                className={cx("size-5 rounded-full", color === value && "ring-2 ring-fg ring-offset-2 ring-offset-panel")}
              />
            ))}
          </div>
        </div>
      </div>
    </Modal>
  );
}
