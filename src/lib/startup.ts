// 启动项与服务：列表、选中的项、正在做的操作、AI 的解释。都只和这台电脑有关，不进仓库。

import { create } from "zustand";
import { confirm } from "../components/dialogs";
import { api } from "./api";
import { type Key, errorText, translate } from "./i18n";
import { useApp } from "./store";
import type { Explanation, StartupAction, StartupItem, StartupListing } from "./types";

interface StartupState {
  /** 还没列出来时是 null。 */
  items: StartupItem[] | null;
  /** 列的时候遇到的问题，比如 `login_items_denied`。 */
  notes: string[];
  loading: boolean;
  /** 上次列完的时间（毫秒），窗口回到前台时用来决定要不要重新列。 */
  scannedAt: number;
  /** 列表出错的原因（整个列不出来时）。 */
  error: string | null;
  selectedId: string | null;
  /** 正在做的操作：id → 操作。做完之前那一行的开关、按钮都不能再点。 */
  busy: Record<string, StartupAction>;
  /** AI 的解释，按 id。解释本身记在仓库的缓存里，这里只是这次运行里取过的。 */
  explanations: Record<string, Explanation>;
  explaining: Record<string, boolean>;
  select: (id: string | null) => void;
  load: () => Promise<void>;
}

export const useStartup = create<StartupState>()((set, get) => ({
  items: null,
  notes: [],
  loading: false,
  scannedAt: 0,
  error: null,
  selectedId: null,
  busy: {},
  explanations: {},
  explaining: {},
  select: (id) => set({ selectedId: id }),
  load: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      adopt(await api.listStartup());
      set({ error: null });
    } catch (error) {
      set({ error: String(error) });
      // 第一次就列不出来时给一个空列表，界面不要一直转圈
      if (get().items === null) set({ items: [] });
    } finally {
      set({ loading: false, scannedAt: Date.now() });
    }
  },
}));

function adopt(listing: StartupListing) {
  const selected = useStartup.getState().selectedId;
  useStartup.setState({
    items: listing.items,
    notes: listing.notes,
    selectedId: selected && listing.items.some((item) => item.id === selected) ? selected : null,
  });
}

function language() {
  return useApp.getState().library?.settings.language ?? "zh-CN";
}

const tr = (key: Key, vars?: Record<string, string | number>) => translate(language(), key, vars);

/** 会让它停下来、或者以后不再自动运行的操作。系统自带的项做这些之前要再确认一次。 */
function weakens(action: StartupAction): boolean {
  return action === "disable" || action === "stop" || action === "type:disabled" || action === "type:manual";
}

/**
 * 对一项做一件事。系统自带的项停用、停止前先确认；需要管理员权限时系统会弹出授权提示。
 * 做完用重新列出来的结果替换列表，并提示做了什么。返回是否成功。
 */
export async function startupAct(item: StartupItem, action: StartupAction): Promise<boolean> {
  const state = useStartup.getState();
  if (state.busy[item.id]) return false;
  if (item.system && weakens(action)) {
    const { ok } = await confirm({
      title: tr("startup.systemConfirm.title", { name: item.name }),
      body: tr("startup.systemConfirm.body"),
      confirmLabel: tr(actionKey(action)),
      danger: true,
    });
    if (!ok) return false;
  }
  useStartup.setState({ busy: { ...useStartup.getState().busy, [item.id]: action } });
  try {
    adopt(await api.startupAction(item.id, action));
    useApp.getState().notify(tr(doneKey(action), { name: item.name, type: startTypeLabel(action.replace("type:", "")) }));
    return true;
  } catch (error) {
    // 在系统的授权提示里点了取消：不算出错，安静地退回去
    if (String(error) === "cancelled") return false;
    useApp.getState().notify(errorText(language(), error), "error");
    // 可能做了一半（停用了但没停下来），重新列一遍看看现在的样子
    void useStartup.getState().load();
    return false;
  } finally {
    const { [item.id]: _done, ...rest } = useStartup.getState().busy;
    useStartup.setState({ busy: rest });
  }
}

/** 以前让 AI 解释过的话，把那次的解释拿出来（只读缓存，不问 AI）。 */
export async function loadExplanation(item: StartupItem) {
  if (useStartup.getState().explanations[item.id]) return;
  try {
    const found = await api.aiExplainStartup(item.id, "cached");
    if (found) useStartup.setState({ explanations: { ...useStartup.getState().explanations, [item.id]: found } });
  } catch {
    // 缓存读不到就当没有
  }
}

/** 让 AI 解释这一项。有记下的解释就直接用；`refresh` 为真（点了“重新解释”）才重新问。 */
export async function explainStartup(item: StartupItem, refresh = false) {
  const state = useStartup.getState();
  if (state.explaining[item.id]) return;
  useStartup.setState({ explaining: { ...state.explaining, [item.id]: true } });
  try {
    const found = await api.aiExplainStartup(item.id, refresh ? "refresh" : "auto");
    if (found) useStartup.setState({ explanations: { ...useStartup.getState().explanations, [item.id]: found } });
  } catch (error) {
    useApp.getState().notify(errorText(language(), error), "error");
  } finally {
    const { [item.id]: _done, ...rest } = useStartup.getState().explaining;
    useStartup.setState({ explaining: rest });
  }
}

function actionKey(action: StartupAction): Key {
  if (action === "disable") return "startup.disable";
  if (action === "stop") return "startup.stop";
  return "startup.changeType";
}

function doneKey(action: StartupAction): Key {
  switch (action) {
    case "enable":
      return "startup.done.enable";
    case "disable":
      return "startup.done.disable";
    case "start":
      return "startup.done.start";
    case "stop":
      return "startup.done.stop";
    case "restart":
      return "startup.done.restart";
    default:
      return "startup.done.type";
  }
}

/** 启动方式的名字：Windows 的 auto / delayed / manual / disabled，systemd 的 enabled / static / masked …，不认识的原样显示。 */
export function startTypeLabel(type: string): string {
  const key = `startup.type.${type}` as Key;
  const text = translate(language(), key);
  return text === key ? type : text;
}

/** 多少秒换成“1 小时”“30 分钟”这样。 */
function interval(seconds: number): string {
  if (seconds % 86400 === 0) return tr("startup.every.day", { n: seconds / 86400 });
  if (seconds % 3600 === 0) return tr("startup.every.hour", { n: seconds / 3600 });
  if (seconds % 60 === 0) return tr("startup.every.minute", { n: seconds / 60 });
  return tr("startup.every.second", { n: seconds });
}

/** 什么时候运行，写成一句话。服务的“加载时”是开机，启动项的是登录。 */
export function triggerText(item: StartupItem): string[] {
  return item.triggers.map((trigger) => {
    if (trigger.startsWith("interval:")) return interval(Number(trigger.slice(9)) || 0);
    if (trigger === "load") return tr(item.kind === "service" ? "startup.when.boot" : "startup.when.login");
    const key = `startup.when.${trigger}` as Key;
    const text = tr(key);
    return text === key ? trigger : text;
  });
}

/** 列表里名字下面那一行：自带的说明，没有时写属于哪个应用（和名字一样时不写），再没有就是命令。 */
export function subtitle(item: StartupItem): string {
  if (item.description && item.description !== item.name) return item.description;
  if (item.app && item.app !== item.name) return tr("startup.belongsTo", { app: item.app });
  return item.command || item.location;
}
