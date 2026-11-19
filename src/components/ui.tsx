// 界面的基础零件。所有尺寸和颜色都在这里统一：控件高 28px、圆角 6px、图标 16px。

import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import * as Dialog from "@radix-ui/react-dialog";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { Check, LoaderCircle, type LucideIcon, X } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";
import { useT } from "../lib/store";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export const ICON = { size: 16, strokeWidth: 1.75 } as const;

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  busy?: boolean;
}

export function Button({ variant = "secondary", busy, className, children, disabled, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      disabled={disabled || busy}
      className={cx(
        "inline-flex h-7 shrink-0 items-center justify-center gap-1.5 rounded-md px-2.5 text-[13px] font-medium whitespace-nowrap transition-colors",
        "disabled:pointer-events-none disabled:opacity-50",
        variant === "primary" && "bg-accent-strong text-white hover:brightness-110",
        variant === "secondary" && "border border-line-strong bg-hover text-fg hover:bg-active",
        variant === "ghost" && "text-fg-2 hover:bg-hover hover:text-fg",
        variant === "danger" && "bg-danger text-white hover:brightness-110",
        className,
      )}
      {...rest}
    >
      {busy && <LoaderCircle size={14} strokeWidth={2} className="animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  active?: boolean;
}

export function IconButton({ label, active, className, children, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      className={cx(
        "inline-flex size-7 shrink-0 items-center justify-center rounded-md transition-colors",
        "disabled:pointer-events-none disabled:opacity-40",
        active ? "bg-accent-soft text-accent" : "text-fg-2 hover:bg-hover hover:text-fg",
        className,
      )}
      {...rest}
    >
      {children}
    </button>
  );
}

const fieldClass =
  "h-7 rounded-md border px-2.5 text-[13px] text-fg outline-none transition-colors " +
  "placeholder:text-fg-3 focus:border-accent focus:ring-1 focus:ring-accent disabled:opacity-50";

export function TextInput({ className, ...rest }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      type="text"
      autoComplete="off"
      spellCheck={false}
      className={cx(fieldClass, "w-full border-line-strong bg-bg", className)}
      {...rest}
    />
  );
}

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  /** 工具栏里用的样式：宽度随内容，没有边框。 */
  compact?: boolean;
}

export function Select({ compact, className, children, ...rest }: SelectProps) {
  return (
    <select
      className={cx(fieldClass, "pr-7", compact ? "shrink-0 border-transparent bg-hover" : "w-full border-line-strong bg-bg", className)}
      {...rest}
    >
      {children}
    </select>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-[12px] font-medium text-fg-2">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-[12px] leading-relaxed text-fg-3">{hint}</span>}
    </label>
  );
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="radiogroup" className="inline-flex rounded-md border border-line-strong bg-bg p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className={cx(
            "h-6 rounded-[5px] px-2.5 text-[12px] font-medium whitespace-nowrap transition-colors",
            option.value === value ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Checkbox({ checked, onChange, children }: { checked: boolean; onChange: (checked: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-start gap-2 text-[13px]">
      <button
        type="button"
        role="checkbox"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={cx(
          "mt-px flex size-4 shrink-0 items-center justify-center rounded-[4px] border transition-colors",
          checked ? "border-accent-strong bg-accent-strong text-white" : "border-line-strong bg-bg",
        )}
      >
        {checked && <Check size={12} strokeWidth={3} aria-hidden />}
      </button>
      <span>{children}</span>
    </label>
  );
}

export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-full transition-colors",
        checked ? "bg-accent-strong" : "bg-active ring-1 ring-line-strong ring-inset",
      )}
    >
      <span className={cx("inline-block size-3.5 rounded-full bg-white shadow-sm transition-transform", checked ? "translate-x-4" : "translate-x-0.5")} />
    </button>
  );
}

/** 灰色小标签：平台、架构、存储模式。 */
export function Chip({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cx("inline-flex h-[18px] shrink-0 items-center rounded-[4px] bg-hover px-1.5 text-[11px] text-fg-2", className)}>
      {children}
    </span>
  );
}

export function EmptyState({ icon: Icon, title, body, action }: { icon: LucideIcon; title: string; body?: string; action?: ReactNode }) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 px-8 text-center">
      <Icon size={28} strokeWidth={1.5} className="mb-1 text-fg-4" aria-hidden />
      <div className="text-[13px] font-medium text-fg-2">{title}</div>
      {body && <div className="max-w-[320px] text-[12px] leading-relaxed text-fg-3">{body}</div>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

// ── 对话框 ───────────────────────────────────────────────────

export function Modal({
  open,
  onClose,
  title,
  width = 420,
  children,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  width?: number;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const t = useT();
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 animate-fade bg-overlay" />
        <Dialog.Content
          aria-describedby={undefined}
          style={{ width }}
          className="fixed top-1/2 left-1/2 z-50 flex max-h-[86vh] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 animate-pop flex-col rounded-xl bg-panel shadow-pop outline-none"
        >
          <div className="flex h-11 shrink-0 items-center justify-between pr-2 pl-4">
            <Dialog.Title className="text-[13px] font-semibold">{title}</Dialog.Title>
            <Dialog.Close asChild>
              <IconButton label={t("common.close")}>
                <X {...ICON} />
              </IconButton>
            </Dialog.Close>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">{children}</div>
          {footer && <div className="flex shrink-0 justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

// ── 菜单 ─────────────────────────────────────────────────────

const menuClass = "z-50 min-w-[168px] animate-pop rounded-lg bg-panel p-1 shadow-pop outline-none";
const itemClass =
  "flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-[13px] outline-none select-none " +
  "data-[highlighted]:bg-active data-[disabled]:pointer-events-none data-[disabled]:opacity-40";

export interface MenuEntry {
  label: string;
  icon?: LucideIcon;
  danger?: boolean;
  disabled?: boolean;
  /** 在这一项前面加一条分隔线。 */
  separated?: boolean;
  onSelect: () => void;
}

function entries(list: MenuEntry[], Item: typeof DropdownPrimitive.Item, Separator: typeof DropdownPrimitive.Separator) {
  return list.map((entry) => (
    <div key={entry.label}>
      {entry.separated && <Separator className="mx-1 my-1 h-px bg-line" />}
      <Item
        disabled={entry.disabled}
        onSelect={entry.onSelect}
        className={cx(itemClass, entry.danger ? "text-danger" : "text-fg")}
      >
        {entry.icon && <entry.icon size={15} strokeWidth={1.75} className={entry.danger ? "" : "text-fg-2"} aria-hidden />}
        {entry.label}
      </Item>
    </div>
  ));
}

/** 点按钮弹出的菜单。`children` 是触发它的按钮。 */
export function Dropdown({ items, align = "end", children }: { items: MenuEntry[]; align?: "start" | "end"; children: ReactNode }) {
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>{children}</DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align={align} sideOffset={4} className={menuClass}>
          {entries(items, DropdownPrimitive.Item, DropdownPrimitive.Separator)}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

/** 右键菜单。`children` 是可以被右键的区域。 */
export function ContextMenu({ items, children }: { items: MenuEntry[]; children: ReactNode }) {
  return (
    <ContextMenuPrimitive.Root>
      <ContextMenuPrimitive.Trigger asChild>{children}</ContextMenuPrimitive.Trigger>
      <ContextMenuPrimitive.Portal>
        <ContextMenuPrimitive.Content className={menuClass}>
          {entries(
            items,
            ContextMenuPrimitive.Item as typeof DropdownPrimitive.Item,
            ContextMenuPrimitive.Separator as typeof DropdownPrimitive.Separator,
          )}
        </ContextMenuPrimitive.Content>
      </ContextMenuPrimitive.Portal>
    </ContextMenuPrimitive.Root>
  );
}
