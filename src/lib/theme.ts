import { t } from "./i18n";
import { api } from "./ipc";

export type ThemeMode = "system" | "light" | "dark";
/** What the user picked, versus what is actually on screen right now. */
export type ResolvedTheme = "light" | "dark";

const DARK_QUERY = "(prefers-color-scheme: dark)";
/** Boot-time hint only; SQLite stays the setting of record. See index.html. */
const CACHE_KEY = "stillenough-theme";

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia(DARK_QUERY).matches;
}

export function resolveTheme(mode: ThemeMode): ResolvedTheme {
  if (mode === "light") return "light";
  if (mode === "dark") return "dark";
  return systemPrefersDark() ? "dark" : "light";
}

const themeOptionValues = (): { value: ThemeMode; label: string; hint: string }[] => [
  { value: "system", label: t("common.system"), hint: t("theme.systemHelp") },
  { value: "light", label: t("theme.light"), hint: t("theme.lightHelp") },
  { value: "dark", label: t("theme.dark"), hint: t("theme.darkHelp") },
];

export function themeOptions(): { value: ThemeMode; label: string; hint: string }[] {
  return themeOptionValues();
}

/**
 * Paints the resolved theme and keeps the native window chrome in step.
 *
 * `data-theme` on <html> is the single switch the stylesheet reads, so the two
 * windows and every component agree without passing anything down.
 */
export function applyTheme(mode: ThemeMode): ResolvedTheme {
  const resolved = resolveTheme(mode);
  document.documentElement.dataset.theme = resolved;
  try {
    localStorage.setItem(CACHE_KEY, mode);
  } catch {
    // Private mode or a locked profile; the next paint just re-resolves.
  }
  // Only the desktop window has native chrome to repaint.
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    void api.syncNativeTheme(resolved === "dark").catch(() => {
      // A repainted title bar is cosmetic; the web content is already correct.
    });
  }
  return resolved;
}

/** Watches the OS appearance, but only while the user asked to follow it. */
export function watchSystemTheme(mode: () => ThemeMode): () => void {
  if (typeof window === "undefined") return () => {};
  const media = window.matchMedia(DARK_QUERY);
  const handler = () => {
    if (mode() === "system") applyTheme("system");
  };
  media.addEventListener("change", handler);
  return () => media.removeEventListener("change", handler);
}
