import { readonly, shallowRef } from "vue";
import { messages, type Locale } from "./messages";
import { readAndMigrateLocalStorageValue } from "@/utils/storage";

const storageKey = "lannook.locale";
const legacyStorageKeys = ["lynqo.locale"] as const;
const supportedLocales: Locale[] = ["zh-CN", "en-US"];

function readStoredLocale(): Locale {
  if (typeof window === "undefined") return "zh-CN";
  const stored = readAndMigrateLocalStorageValue(storageKey, legacyStorageKeys);
  if (supportedLocales.includes(stored as Locale)) return stored as Locale;
  const browserLocale = window.navigator.languages?.[0] ?? window.navigator.language;
  return browserLocale?.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

const activeLocale = shallowRef<Locale>(readStoredLocale());

function applyDocumentLocale(locale: Locale) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
}

function interpolate(message: string, params: Record<string, string | number>) {
  return message.replace(/\{(\w+)\}/g, (match, key: string) =>
    params[key] === undefined ? match : String(params[key])
  );
}

export function translate(key: string, params: Record<string, string | number> = {}) {
  const message = messages[activeLocale.value][key] ?? messages["zh-CN"][key];
  if (message === undefined) {
    // audit-32: never render a raw key to users. In dev builds the marker
    // makes missing translations obvious; production falls back to the key
    // itself only as a last resort.
    if (import.meta.env.DEV) return `[missing:${key}]`;
    return key;
  }
  return interpolate(message, params);
}

export function setLocale(locale: Locale) {
  activeLocale.value = locale;
  if (typeof window !== "undefined") {
    window.localStorage.setItem(storageKey, locale);
  }
  applyDocumentLocale(locale);
}

export function initializeLocale() {
  applyDocumentLocale(activeLocale.value);
}

export function getCurrentLocale(): Locale {
  return activeLocale.value;
}

export function useLocale() {
  return {
    locale: readonly(activeLocale),
    setLocale,
    t: translate,
  };
}

export type { Locale } from "./messages";
