// AI 服务的预设，和“这台电脑上 AI 能不能用”的判断。
//
// 都是 OpenAI 兼容接口，区别只在地址和去哪里申请密钥。模型名更新得快，这里只给少数长期稳定的别名作默认值，
// 其余在设置里从服务的模型列表里选。

import type { AiSettings, Language } from "./types";

export interface AiPreset {
  id: string;
  /** 中文界面和其他语言界面的名字。 */
  name: [zh: string, other: string];
  baseUrl: string;
  /** 推荐的模型，空字符串表示让用户从列表里选。 */
  model: string;
  /** 在哪里申请 API Key（本机服务是下载页）。 */
  keyUrl: string;
  /** 运行在这台电脑上，不需要密钥，内容不出电脑。 */
  local?: boolean;
}

export const AI_PRESETS: AiPreset[] = [
  { id: "deepseek", name: ["DeepSeek", "DeepSeek"], baseUrl: "https://api.deepseek.com", model: "deepseek-chat", keyUrl: "https://platform.deepseek.com/api_keys" },
  { id: "qwen", name: ["通义千问", "Qwen (China)"], baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-plus", keyUrl: "https://bailian.console.aliyun.com/" },
  { id: "qwen_intl", name: ["通义千问（国际站）", "Qwen (International)"], baseUrl: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-plus", keyUrl: "https://modelstudio.console.alibabacloud.com/" },
  { id: "moonshot", name: ["Kimi", "Kimi"], baseUrl: "https://api.moonshot.cn/v1", model: "", keyUrl: "https://platform.moonshot.cn/console/api-keys" },
  { id: "zhipu", name: ["智谱 GLM", "Zhipu GLM"], baseUrl: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4.7", keyUrl: "https://open.bigmodel.cn/usercenter/apikeys" },
  { id: "siliconflow", name: ["硅基流动", "SiliconFlow"], baseUrl: "https://api.siliconflow.cn/v1", model: "", keyUrl: "https://cloud.siliconflow.cn/account/ak" },
  { id: "openai", name: ["OpenAI", "OpenAI"], baseUrl: "https://api.openai.com/v1", model: "", keyUrl: "https://platform.openai.com/api-keys" },
  { id: "gemini", name: ["Gemini", "Gemini"], baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", model: "", keyUrl: "https://aistudio.google.com/apikey" },
  { id: "openrouter", name: ["OpenRouter", "OpenRouter"], baseUrl: "https://openrouter.ai/api/v1", model: "", keyUrl: "https://openrouter.ai/keys" },
  { id: "ollama", name: ["Ollama（本机）", "Ollama (this computer)"], baseUrl: "http://localhost:11434/v1", model: "", keyUrl: "https://ollama.com/download", local: true },
  { id: "lmstudio", name: ["LM Studio（本机）", "LM Studio (this computer)"], baseUrl: "http://localhost:1234/v1", model: "", keyUrl: "https://lmstudio.ai/", local: true },
];

export function presetOf(id: string): AiPreset | undefined {
  return AI_PRESETS.find((preset) => preset.id === id);
}

export function presetName(preset: AiPreset, language: Language): string {
  return language.toLowerCase().startsWith("zh") ? preset.name[0] : preset.name[1];
}

/** 服务在这台电脑上（Ollama、LM Studio）：不需要密钥。和 `crates/core/src/ai.rs` 的 `is_local` 一致。 */
export function isLocalUrl(baseUrl: string): boolean {
  const rest = baseUrl.trim().replace(/^[a-z]+:\/\//i, "");
  const host = (rest.startsWith("[") ? rest.slice(1).split("]")[0] : rest.split(/[/:]/)[0]).toLowerCase();
  return host === "localhost" || host === "127.0.0.1" || host === "::1" || host.endsWith(".localhost");
}

/** 设置里填了地址和模型。 */
export function aiConfigured(ai: AiSettings | undefined): ai is AiSettings {
  return Boolean(ai?.baseUrl.trim() && ai.model.trim());
}

/** 编辑结束后只显示末尾 6 个字符。短密钥整串遮蔽。 */
export function maskAiKey(key: string): string {
  const characters = Array.from(key.trim());
  return characters.length > 6 ? `••••••${characters.slice(-6).join("")}` : "••••••";
}

/** 这台电脑上能用 AI：地址、模型都有，不是本机服务时钥匙串里还存着密钥。 */
export function aiUsable(ai: AiSettings | undefined, keySaved: boolean): boolean {
  return aiConfigured(ai) && (keySaved || isLocalUrl(ai.baseUrl));
}

/** 设置里显示的服务名：预设的名字，自定义的显示地址的主机名。 */
export function serviceLabel(ai: AiSettings, language: Language): string {
  const preset = presetOf(ai.preset);
  if (preset) return presetName(preset, language);
  return ai.baseUrl.replace(/^[a-z]+:\/\//i, "").split("/")[0] || ai.baseUrl;
}
