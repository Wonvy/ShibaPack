import { computed, ref } from "vue";
import zhCN, { type Dictionary } from "./locales/zh-CN";
import enUS from "./locales/en-US";

export type LocaleCode = "zh-CN" | "en-US";

export interface LanguageOption {
  code: LocaleCode;
  name: string;
}

export const languages: LanguageOption[] = [
  { code: "zh-CN", name: "简体中文" },
  { code: "en-US", name: "English" },
];

type StringMap = Record<string, unknown>;

const dictionaries: Record<LocaleCode, StringMap> = {
  "zh-CN": zhCN as unknown as StringMap,
  "en-US": enUS as unknown as StringMap,
};

const current = ref<LocaleCode>("zh-CN");
const version = ref(0);

export const locale = computed(() => current.value);
export const localeName = computed(
  () => languages.find((item) => item.code === current.value)?.name ?? current.value,
);

function lookup(table: StringMap, key: string): string | undefined {
  if (key in table) {
    const value = table[key];
    return typeof value === "string" ? value : undefined;
  }
  const found = key.split(".").reduce<unknown>((node, part) => {
    if (node && typeof node === "object" && part in (node as StringMap)) {
      return (node as StringMap)[part];
    }
    return undefined;
  }, table);
  return typeof found === "string" ? found : undefined;
}

/** 响应式翻译函数：在模板中直接调用 t('key') 即可。 */
export function t(key: string, params?: Record<string, string | number>): string {
  void version.value;
  const active = dictionaries[current.value] ?? dictionaries["zh-CN"];
  const fallback = dictionaries["zh-CN"];
  let text = lookup(active, key) ?? lookup(fallback, key) ?? key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      text = text.replaceAll(`{${name}}`, String(value));
    }
  }
  return text;
}

export function setLocale(code: string) {
  current.value = code in dictionaries ? (code as LocaleCode) : "zh-CN";
  version.value += 1;
}

/** 导出当前语言为 JSON，用户可翻译后重新导入。 */
export function exportLanguage(code: LocaleCode = current.value): string {
  return JSON.stringify(
    { _locale: code, _name: localeName.value, ...dictionaries[code] },
    null,
    2,
  );
}

export interface ImportResult {
  ok: boolean;
  code?: LocaleCode;
  message: string;
}

/** 导入语言文件。支持带 _locale 的完整文件，也支持纯键值对字典。 */
export function importLanguage(json: string): ImportResult {
  let parsed: StringMap;
  try {
    const value: unknown = JSON.parse(json);
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { ok: false, message: "root" };
    }
    parsed = value as StringMap;
  } catch {
    return { ok: false, message: "json" };
  }

  const rawCode = typeof parsed._locale === "string" ? parsed._locale : current.value;
  const { _locale, _name, ...entries } = parsed;
  void _locale;
  void _name;

  const code: LocaleCode = rawCode in dictionaries ? (rawCode as LocaleCode) : current.value;
  if (Object.keys(entries).length === 0) return { ok: false, message: "empty" };

  dictionaries[code] = { ...dictionaries[code], ...entries };
  current.value = code;
  version.value += 1;
  return { ok: true, code, message: "ok" };
}

export function resetLanguage(code: LocaleCode) {
  dictionaries[code] =
    code === "zh-CN"
      ? (zhCN as unknown as StringMap)
      : (enUS as unknown as StringMap);
}

export type { Dictionary };
