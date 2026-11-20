// 界面的基础零件。所有尺寸和颜色都在这里统一：控件高 28px、圆角 6px、图标 16px。

import * as ContextMenuPrimitive from "@radix-ui/react-context-menu";
import * as Dialog from "@radix-ui/react-dialog";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown, LoaderCircle, type LucideIcon, X } from "lucide-react";
import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode, SelectHTMLAttributes } from "react";
import { useT } from "../lib/store";

export function cx(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}

export const ICON = { size: 16, strokeWidth: 1.75 } as const;

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /**
   * `caution`：普通按钮的样子、红字，用在删除、清空这类操作上。
   * `soft`：淡强调色底，用在列表里的“安装”。`quiet`：平时灰字，悬停变红，用在列表里的“卸载”。
   */
  variant?: "primary" | "secondary" | "ghost" | "danger" | "caution" | "soft" | "quiet";
  /** `sm`：24px 高、12px 字，放在列表行和卡片里。 */
  size?: "md" | "sm";
  busy?: boolean;
}

export function Button({ variant = "secondary", size = "md", busy, className, children, disabled, ...rest }: ButtonProps) {
  return (
    <button
      type="button"
      disabled={disabled || busy}
      className={cx(
        "inline-flex shrink-0 items-center justify-center gap-1.5 rounded-md font-medium whitespace-nowrap transition-colors",
        size === "sm" ? "h-6 px-2 text-[12px]" : "h-7 px-2.5 text-[13px]",
        "disabled:pointer-events-none disabled:opacity-50",
        variant === "primary" && "bg-accent-strong text-white hover:brightness-110",
        variant === "secondary" && "border border-line-strong bg-hover text-fg hover:bg-active",
        variant === "ghost" && "text-fg-2 hover:bg-hover hover:text-fg",
        variant === "danger" && "bg-danger text-white hover:brightness-110",
        variant === "caution" && "border border-line-strong bg-hover text-danger hover:bg-active",
        variant === "soft" && "bg-accent-soft text-accent hover:bg-[color-mix(in_srgb,var(--accent)_26%,transparent)]",
        variant === "quiet" && "text-fg-3 hover:bg-hover hover:text-danger",
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

/** 只有图标的分段按钮，用来切换视图。选中项是中性色，不用强调色。 */
export function IconSegmented<T extends string>({
  value,
  options,
  onChange,
}: {
  value: T;
  options: { value: T; label: string; icon: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="radiogroup" className="inline-flex shrink-0 rounded-md border border-line-strong bg-bg p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          aria-label={option.label}
          title={option.label}
          onClick={() => onChange(option.value)}
          className={cx(
            "inline-flex size-6 items-center justify-center rounded-[5px] transition-colors",
            option.value === value ? "bg-active text-fg" : "text-fg-3 hover:text-fg-2",
          )}
        >
          {option.icon}
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

export function Switch({ checked, onChange, label, disabled }: { checked: boolean; onChange: (checked: boolean) => void; label: string; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative inline-flex h-[18px] w-8 shrink-0 items-center rounded-full transition-colors disabled:opacity-50",
        checked ? "bg-accent-strong" : "bg-active ring-1 ring-line-strong ring-inset",
      )}
    >
      <span className={cx("inline-block size-3.5 rounded-full bg-white shadow-sm transition-transform", checked ? "translate-x-4" : "translate-x-0.5")} />
    </button>
  );
}

/**
 * 太长时从中间省略，保留开头和结尾。文件名的版本号、架构、扩展名多在结尾，
 * 从末尾截断会把最有用的部分截掉。
 */
export function MiddleTruncate({ text, tail = 16, title = text, className }: { text: string; tail?: number; title?: string | null; className?: string }) {
  if (text.length <= tail + 4) {
    return (
      <span className={cx("block truncate", className)} title={title ?? undefined}>
        {text}
      </span>
    );
  }
  return (
    <span className={cx("flex min-w-0", className)} title={title ?? undefined}>
      <span className="truncate">{text.slice(0, text.length - tail)}</span>
      <span className="shrink-0 whitespace-pre">{text.slice(text.length - tail)}</span>
    </span>
  );
}

/** 灰色小标签：平台、架构、存储模式。 */
export function Chip({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <span className={cx("inline-flex h-5 shrink-0 items-center rounded-[4px] bg-hover px-1.5 text-[12px] text-fg-3", className)}>
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

/**
 * 设置窗口里的一行：左边是名字，右边是开关或按钮（`children`），下面是说明（`hint`）和更多内容（`below`，比如列表）。
 * 行与行之间一条细线。
 */
export function SettingRow({ label, hint, children, below }: { label: string; hint?: ReactNode; children?: ReactNode; below?: ReactNode }) {
  return (
    <div className="border-b border-line py-3 last:border-b-0">
      <div className="flex min-h-7 flex-wrap items-center justify-between gap-x-4 gap-y-2">
        <span className="text-[13px]">{label}</span>
        {children}
      </div>
      {hint && <p className="m-0 mt-2 text-[12px] leading-relaxed text-fg-3">{hint}</p>}
      {below && <div className="mt-2.5">{below}</div>}
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
  bare = false,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  width?: number;
  children: ReactNode;
  footer?: ReactNode;
  /** 内容不留边距、不自己滚动，由内容自己排（比如左边是选项卡的设置窗口）。 */
  bare?: boolean;
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
          <div className={bare ? "flex min-h-0 flex-1 flex-col" : "min-h-0 flex-1 overflow-y-auto px-4 pb-4"}>{children}</div>
          {footer && <div className="flex shrink-0 justify-end gap-2 border-t border-line px-4 py-3">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

// ── 菜单 ─────────────────────────────────────────────────────

export const menuClass = "z-50 min-w-[168px] animate-pop rounded-lg bg-panel p-1 shadow-pop outline-none";
export const itemClass =
  "flex h-7 cursor-default items-center gap-2 rounded-md px-2 text-[13px] outline-none select-none " +
  "data-[highlighted]:bg-active data-[disabled]:pointer-events-none data-[disabled]:opacity-40";

export interface MenuEntry {
  label: string;
  icon?: LucideIcon;
  danger?: boolean;
  disabled?: boolean;
  /** 在这一项前面加一条分隔线。 */
  separated?: boolean;
  /** 给了就是一个勾选项：前面显示对勾，点了菜单不关，可以接着勾别的。 */
  checked?: boolean;
  onSelect: () => void;
}

function entries(list: MenuEntry[], Item: typeof DropdownPrimitive.Item, Separator: typeof DropdownPrimitive.Separator) {
  return list.map((entry) => {
    const checkable = entry.checked !== undefined;
    return (
      <div key={entry.label}>
        {entry.separated && <Separator className="mx-1 my-1 h-px bg-line" />}
        <Item
          disabled={entry.disabled}
          onSelect={(event) => {
            if (checkable) event.preventDefault();
            entry.onSelect();
          }}
          {...(checkable ? { role: "menuitemcheckbox", "aria-checked": entry.checked } : {})}
          className={cx(itemClass, entry.danger ? "text-danger" : "text-fg")}
        >
          {checkable ? (
            <span className="flex size-[15px] shrink-0 items-center justify-center text-accent" aria-hidden>
              {entry.checked && <Check size={14} strokeWidth={2.25} />}
            </span>
          ) : (
            entry.icon && <entry.icon size={15} strokeWidth={1.75} className={entry.danger ? "" : "text-fg-2"} aria-hidden />
          )}
          {entry.label}
        </Item>
      </div>
    );
  });
}

/** 点按钮弹出的菜单。`children` 是触发它的按钮。 */
export function Dropdown({ items, align = "end", side = "bottom", children }: {
  items: MenuEntry[];
  align?: "start" | "end";
  side?: "top" | "bottom";
  children: ReactNode;
}) {
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>{children}</DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align={align} side={side} sideOffset={4} className={menuClass}>
          {entries(items, DropdownPrimitive.Item, DropdownPrimitive.Separator)}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}

/** 右键菜单。`children` 是可以被右键的区域。 */
export function ContextMenu({
  items,
  children,
  onCloseAutoFocus,
}: {
  items: MenuEntry[];
  children: ReactNode;
  onCloseAutoFocus?: (event: Event) => void;
}) {
  return (
    <ContextMenuPrimitive.Root>
      <ContextMenuPrimitive.Trigger asChild>{children}</ContextMenuPrimitive.Trigger>
      <ContextMenuPrimitive.Portal>
        <ContextMenuPrimitive.Content className={menuClass} onCloseAutoFocus={onCloseAutoFocus}>
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

export interface FilterOption<T extends string> {
  value: T;
  label: string;
  icon?: ReactNode;
}

/**
 * 工具栏上的筛选菜单：按钮上显示当前选项，选项可以有图标。
 */
export function FilterMenu<T extends string>({
  label,
  value,
  options,
  onChange,
  textClassName,
}: {
  label: string;
  value: T;
  options: FilterOption<T>[];
  onChange: (value: T) => void;
  /** 按钮上文字的样式，比如栏窄时藏起来只留图标（`hidden @[540px]:inline`）。 */
  textClassName?: string;
}) {
  const current = options.find((option) => option.value === value) ?? options[0];
  const hasIcons = options.some((option) => option.icon);
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>
        <button
          type="button"
          aria-label={`${label} · ${current.label}`}
          title={`${label} · ${current.label}`}
          className="inline-flex h-7 min-w-0 items-center gap-1.5 rounded-md px-2 text-[13px] whitespace-nowrap text-fg-3 transition-colors outline-none hover:bg-hover hover:text-fg data-[state=open]:bg-hover data-[state=open]:text-fg"
        >
          {current.icon && <span className="flex size-4 shrink-0 items-center justify-center">{current.icon}</span>}
          <span className={cx("truncate", current.icon ? textClassName : undefined)}>{current.label}</span>
          <ChevronDown size={12} strokeWidth={2} aria-hidden className="-ml-0.5 shrink-0 opacity-50" />
        </button>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align="start" sideOffset={4} className={menuClass}>
          <DropdownPrimitive.RadioGroup value={value} onValueChange={(next) => onChange(next as T)}>
            {options.map((option) => (
              <DropdownPrimitive.RadioItem key={option.value} value={option.value} className={cx(itemClass, "pr-2 text-fg")}>
                {hasIcons && <span className="flex size-4 shrink-0 items-center justify-center text-fg-2">{option.icon}</span>}
                <span className="flex-1">{option.label}</span>
                <DropdownPrimitive.ItemIndicator>
                  <Check size={14} strokeWidth={2} className="text-accent" />
                </DropdownPrimitive.ItemIndicator>
              </DropdownPrimitive.RadioItem>
            ))}
          </DropdownPrimitive.RadioGroup>
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}


/**
 * 列表工具栏右边的“视图”菜单：按钮是当前视图的图标，菜单里换视图；`children` 是更多显示选项（排序、分组），
 * 放在视图下面。把视图切换和显示选项收进一个按钮，工具栏上少几个图标。
 */
export function ViewMenu<T extends string>({
  label,
  value,
  options,
  onChange,
  children,
}: {
  label: string;
  value: T;
  options: { value: T; label: string; icon: ReactNode }[];
  onChange: (value: T) => void;
  children?: ReactNode;
}) {
  const current = options.find((option) => option.value === value) ?? options[0];
  return (
    <DropdownPrimitive.Root>
      <DropdownPrimitive.Trigger asChild>
        <IconButton label={`${label} · ${current.label}`} className="data-[state=open]:bg-hover data-[state=open]:text-fg">
          {current.icon}
        </IconButton>
      </DropdownPrimitive.Trigger>
      <DropdownPrimitive.Portal>
        <DropdownPrimitive.Content align="end" sideOffset={4} className={menuClass}>
          <DropdownPrimitive.Label className="px-2 pt-1.5 pb-1 text-[12px] text-fg-3">{label}</DropdownPrimitive.Label>
          <DropdownPrimitive.RadioGroup value={value} onValueChange={(next) => onChange(next as T)}>
            {options.map((option) => (
              <DropdownPrimitive.RadioItem key={option.value} value={option.value} className={cx(itemClass, "pr-2 text-fg")}>
                <span className="flex size-4 items-center justify-center text-fg-2">{option.icon}</span>
                <span className="flex-1">{option.label}</span>
                <DropdownPrimitive.ItemIndicator>
                  <Check size={14} strokeWidth={2} className="text-accent" />
                </DropdownPrimitive.ItemIndicator>
              </DropdownPrimitive.RadioItem>
            ))}
          </DropdownPrimitive.RadioGroup>
          {children && (
            <>
              <DropdownPrimitive.Separator className="mx-1 my-1 h-px bg-line" />
              {children}
            </>
          )}
        </DropdownPrimitive.Content>
      </DropdownPrimitive.Portal>
    </DropdownPrimitive.Root>
  );
}
