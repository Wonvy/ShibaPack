// 确认框和输入框。任何地方都可以 `await confirm(...)` / `await prompt(...)`，界面由 DialogHost 统一画。

import { useEffect, useState } from "react";
import { create } from "zustand";
import { useT } from "../lib/store";
import { Button, Checkbox, Field, Modal, TextInput } from "./ui";

interface ConfirmOptions {
  title: string;
  body?: string;
  confirmLabel?: string;
  danger?: boolean;
  /** 带一个勾选项，比如“同时删除磁盘上的文件”。 */
  checkbox?: { label: string; hintOn: string; hintOff: string; initial?: boolean };
}

interface PromptOptions {
  title: string;
  label: string;
  initial?: string;
  placeholder?: string;
}

type Request =
  | ({ kind: "confirm"; resolve: (result: { ok: boolean; checked: boolean }) => void } & ConfirmOptions)
  | ({ kind: "prompt"; resolve: (value: string | null) => void } & PromptOptions);

const useDialog = create<{ request: Request | null }>(() => ({ request: null }));

export function confirm(options: ConfirmOptions): Promise<{ ok: boolean; checked: boolean }> {
  return new Promise((resolve) => show({ kind: "confirm", resolve, ...options }));
}

export function prompt(options: PromptOptions): Promise<string | null> {
  return new Promise((resolve) => show({ kind: "prompt", resolve, ...options }));
}

/** 换上新的对话框。前一个还开着的话当作取消，等它结果的地方不会一直挂着。 */
function show(next: Request) {
  const previous = useDialog.getState().request;
  if (previous?.kind === "confirm") previous.resolve({ ok: false, checked: false });
  if (previous?.kind === "prompt") previous.resolve(null);
  useDialog.setState({ request: next });
}

export function DialogHost() {
  const t = useT();
  const request = useDialog((state) => state.request);
  const [checked, setChecked] = useState(false);
  const [text, setText] = useState("");

  useEffect(() => {
    if (request?.kind === "confirm") setChecked(request.checkbox?.initial ?? false);
    if (request?.kind === "prompt") setText(request.initial ?? "");
  }, [request]);

  if (!request) return null;
  const close = () => useDialog.setState({ request: null });

  if (request.kind === "prompt") {
    const finish = (value: string | null) => {
      close();
      request.resolve(value);
    };
    return (
      <Modal
        open
        onClose={() => finish(null)}
        title={request.title}
        width={360}
        footer={
          <>
            <Button variant="ghost" onClick={() => finish(null)}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" onClick={() => finish(text.trim())}>
              {t("common.confirm")}
            </Button>
          </>
        }
      >
        <Field label={request.label}>
          <TextInput
            autoFocus
            value={text}
            placeholder={request.placeholder}
            onChange={(event) => setText(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && finish(text.trim())}
          />
        </Field>
      </Modal>
    );
  }

  const finish = (ok: boolean) => {
    close();
    request.resolve({ ok, checked });
  };
  return (
    <Modal
      open
      onClose={() => finish(false)}
      title={request.title}
      width={400}
      footer={
        <>
          {/* 删除这类危险操作默认停在“取消”上：按 Delete 再顺手按回车，不会直接删掉 */}
          <Button variant="ghost" onClick={() => finish(false)} autoFocus={request.danger}>
            {t("common.cancel")}
          </Button>
          <Button variant={request.danger ? "danger" : "primary"} onClick={() => finish(true)} autoFocus={!request.danger}>
            {request.confirmLabel ?? t("common.confirm")}
          </Button>
        </>
      }
    >
      {request.body && <p className="m-0 text-[13px] leading-relaxed text-fg-2">{request.body}</p>}
      {request.checkbox && (
        <div className="mt-1">
          <Checkbox checked={checked} onChange={setChecked}>
            {request.checkbox.label}
          </Checkbox>
          <p className="mt-2 mb-0 pl-6 text-[12px] leading-relaxed text-fg-3">
            {checked ? request.checkbox.hintOn : request.checkbox.hintOff}
          </p>
        </div>
      )}
    </Modal>
  );
}
