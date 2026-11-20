// AI 模型的设置：选服务、填地址和密钥、选模型、测试连接。
//
// 地址和模型存进仓库的设置；API Key 只存在这台电脑的系统钥匙串里。

import { CircleCheck, ExternalLink, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { AI_PRESETS, isLocalUrl, presetName, presetOf } from "../lib/ai";
import { api } from "../lib/api";
import { errorText } from "../lib/i18n";
import { useApp, useLanguage, useT } from "../lib/store";
import type { AiSettings } from "../lib/types";
import { Button, Field, Modal, Select, TextInput, cx } from "./ui";

const CUSTOM = "custom";

export function AiDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  // 每次打开都从设置里重新读一遍，上次没保存的改动不留着
  return open ? <Form onClose={onClose} /> : null;
}

/** 测试、取模型列表的结果。 */
type Outcome = { ok: boolean; text: string } | null;

function Form({ onClose }: { onClose: () => void }) {
  const t = useT();
  const language = useLanguage();
  const saved = useApp((state) => state.library!.settings.ai);
  const saveSettings = useApp((state) => state.saveSettings);
  const refreshAiKey = useApp((state) => state.refreshAiKey);
  const notify = useApp((state) => state.notify);
  const attempt = useApp((state) => state.attempt);

  const configured = Boolean(saved.baseUrl.trim());
  const first = AI_PRESETS[0];
  const [preset, setPreset] = useState(configured ? saved.preset || CUSTOM : first.id);
  const [baseUrl, setBaseUrl] = useState(configured ? saved.baseUrl : first.baseUrl);
  const [model, setModel] = useState(configured ? saved.model : first.model);
  const [key, setKey] = useState("");
  /** 这台电脑的钥匙串里有没有表单里这个地址的密钥；还没查完时是 null。 */
  const [keySaved, setKeySaved] = useState<boolean | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [busy, setBusy] = useState<"models" | "test" | "save" | null>(null);
  const [outcome, setOutcome] = useState<Outcome>(null);

  const local = isLocalUrl(baseUrl);
  const info = presetOf(preset);
  const form: AiSettings = { preset, baseUrl: baseUrl.trim(), model: model.trim() };

  // 地址变了就重新看钥匙串里有没有它的密钥（打字时等停下来再查）
  useEffect(() => {
    setKeySaved(null);
    const url = baseUrl.trim();
    if (!url) return setKeySaved(false);
    let alive = true;
    const timer = setTimeout(() => {
      api.aiKeySaved(url).then(
        (found) => alive && setKeySaved(found),
        () => alive && setKeySaved(false),
      );
    }, 300);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [baseUrl]);

  const choosePreset = (id: string) => {
    setPreset(id);
    setOutcome(null);
    setModels([]);
    const next = presetOf(id);
    if (next) {
      setBaseUrl(next.baseUrl);
      setModel(next.model);
    } else if (presetOf(preset)) {
      // 从预设换成自定义：地址留着方便改，模型清空
      setModel("");
    }
  };

  const loadModels = async () => {
    setBusy("models");
    setOutcome(null);
    try {
      const list = await api.aiModels(form, key);
      setModels(list);
      if (!model.trim() && list.length > 0) setModel(list[0]);
      setOutcome({ ok: true, text: t("ai.modelsLoaded", { n: list.length }) });
    } catch (error) {
      setOutcome({ ok: false, text: errorText(language, error) });
    } finally {
      setBusy(null);
    }
  };

  const test = async () => {
    setBusy("test");
    setOutcome(null);
    try {
      const reply = await api.aiTest(form, key);
      setOutcome({ ok: true, text: t("ai.testOk", { reply }) });
    } catch (error) {
      setOutcome({ ok: false, text: errorText(language, error) });
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    setBusy("save");
    try {
      if (key.trim() && !local && (await attempt(() => api.aiSetKey(form.baseUrl, key).then(() => true))) !== true) return;
      await saveSettings({ ai: form });
      await refreshAiKey();
      notify(t("ai.saved"));
      onClose();
    } finally {
      setBusy(null);
    }
  };

  const removeKey = async () => {
    if ((await attempt(() => api.aiSetKey(form.baseUrl, "").then(() => true))) !== true) return;
    setKeySaved(false);
    setKey("");
    await refreshAiKey();
  };

  const turnOff = async () => {
    await saveSettings({ ai: { preset: "", baseUrl: "", model: "" } });
    await refreshAiKey();
    notify(t("ai.off"));
    onClose();
  };

  const canSave = Boolean(form.baseUrl && form.model) && (local || key.trim() !== "" || keySaved === true);
  const link = (url: string, label: string) => (
    <button type="button" onClick={() => void attempt(() => api.openUrl(url))} className="inline-flex items-center gap-1 text-accent hover:underline">
      {label}
      <ExternalLink size={12} strokeWidth={2} aria-hidden />
    </button>
  );

  return (
    <Modal
      open
      onClose={onClose}
      title={t("ai.title")}
      width={480}
      footer={
        <>
          {configured && (
            <Button variant="ghost" className="mr-auto text-danger" onClick={() => void turnOff()}>
              {t("ai.turnOff")}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" busy={busy === "save"} disabled={!canSave || busy !== null} onClick={() => void save()}>
            {t("common.save")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pt-1">
        <Field
          label={t("ai.service")}
          hint={
            info?.local ? (
              <>
                {t("ai.localHint")} {link(info.keyUrl, t("ai.download"))}
              </>
            ) : undefined
          }
        >
          <Select value={preset} onChange={(event) => choosePreset(event.target.value)}>
            {AI_PRESETS.map((item) => (
              <option key={item.id} value={item.id}>
                {presetName(item, language)}
              </option>
            ))}
            <option value={CUSTOM}>{t("ai.custom")}</option>
          </Select>
        </Field>

        <Field label={t("ai.baseUrl")} hint={preset === CUSTOM ? t("ai.baseUrlHint") : undefined}>
          <TextInput
            value={baseUrl}
            placeholder="https://…/v1"
            onChange={(event) => {
              setBaseUrl(event.target.value);
              setOutcome(null);
            }}
          />
        </Field>

        {!local && (
          <Field
            label={t("ai.key")}
            hint={
              <>
                {t("ai.keyHint")} {info && link(info.keyUrl, t("ai.getKey"))}
              </>
            }
          >
            <div className="flex gap-2">
              <TextInput
                type="password"
                value={key}
                placeholder={keySaved ? t("ai.keySaved") : t("ai.keyPlaceholder")}
                onChange={(event) => {
                  setKey(event.target.value);
                  setOutcome(null);
                }}
              />
              {keySaved && (
                <Button variant="ghost" className="shrink-0 text-danger" onClick={() => void removeKey()}>
                  {t("ai.removeKey")}
                </Button>
              )}
            </div>
          </Field>
        )}

        <Field label={t("ai.model")}>
          <div className="flex gap-2">
            <TextInput list="ai-models" value={model} placeholder={t("ai.modelPlaceholder")} onChange={(event) => setModel(event.target.value)} />
            <datalist id="ai-models">
              {models.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
            <Button className="shrink-0" busy={busy === "models"} disabled={!form.baseUrl || busy !== null} onClick={() => void loadModels()}>
              {t("ai.listModels")}
            </Button>
          </div>
        </Field>

        <div className="flex items-start gap-3">
          <Button className="shrink-0" busy={busy === "test"} disabled={!form.baseUrl || !form.model || busy !== null} onClick={() => void test()}>
            {t("ai.test")}
          </Button>
          {outcome && (
            <p className={cx("m-0 flex min-w-0 items-start gap-1.5 pt-1 text-[12px] leading-relaxed break-words", outcome.ok ? "text-success" : "text-danger")}>
              {outcome.ok ? (
                <CircleCheck size={14} strokeWidth={2} aria-hidden className="mt-0.5 shrink-0" />
              ) : (
                <TriangleAlert size={14} strokeWidth={2} aria-hidden className="mt-0.5 shrink-0" />
              )}
              <span className="min-w-0">{outcome.text}</span>
            </p>
          )}
        </div>

        <p className="m-0 text-[12px] leading-relaxed text-fg-3">{t("ai.privacy")}</p>
      </div>
    </Modal>
  );
}
