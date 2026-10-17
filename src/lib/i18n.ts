import { useSyncExternalStore } from "react";
import { messages, type MessageKey } from "./locales";

export type LanguagePreference = "system" | "zh-CN" | "en";
export type Language = "zh-CN" | "en";
let language: Language =
  typeof window !== "undefined" && !navigator.language.toLowerCase().startsWith("zh") ? "en" : "zh-CN";
const listeners = new Set<() => void>();
export const currentLanguage = () => language;
export function applyLanguage(next: Language) {
  if (typeof document !== "undefined") document.documentElement.lang = next;
  if (language === next) return;
  language = next;
  listeners.forEach((listener) => listener());
}
export function useLanguage() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    currentLanguage,
    currentLanguage,
  );
}

/** Named parameters are replaced once; user text is never interpreted as a template. */
export function translate(locale: Language, key: MessageKey, params: Record<string, string | number> = {}) {
  const entry = messages[key];
  if (!Object.prototype.hasOwnProperty.call(messages, key)) return key;
  return entry[locale === "en" ? 1 : 0].replace(/\{(\w+)\}/g, (token, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : token,
  );
}
export function t(key: MessageKey, params?: Record<string, string | number>) {
  return translate(language, key, params);
}
