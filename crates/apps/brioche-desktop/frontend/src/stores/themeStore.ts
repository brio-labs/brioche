import { create } from "zustand";

export type ThemeId = "brio" | "catppuccin-mocha";

export interface ThemeDefinition {
  id: ThemeId;
  label: string;
  description: string;
}

export const THEMES: ThemeDefinition[] = [
  {
    id: "brio",
    label: "Brio",
    description: "Dark botanical Brio palette with cream text and warm translucent controls.",
  },
  {
    id: "catppuccin-mocha",
    label: "Catppuccin Mocha",
    description: "Soft lavender Catppuccin Mocha palette for testing runtime theme switching.",
  },
];

const THEME_STORAGE_KEY = "brioche.theme";
const DEFAULT_THEME: ThemeId = "brio";

function isThemeId(value: unknown): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

function readStoredTheme(): ThemeId {
  if (typeof window === "undefined") return DEFAULT_THEME;
  const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
  return isThemeId(stored) ? stored : DEFAULT_THEME;
}

function applyTheme(theme: ThemeId) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = "dark";
}

function persistTheme(theme: ThemeId) {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(THEME_STORAGE_KEY, theme);
}

export function initializeTheme(): ThemeId {
  const theme = readStoredTheme();
  applyTheme(theme);
  return theme;
}

export function setThemePreference(theme: ThemeId) {
  applyTheme(theme);
  persistTheme(theme);
  useThemeStore.setState({ theme });
}

interface ThemeStore {
  theme: ThemeId;
  setTheme: (theme: ThemeId) => void;
}

export const useThemeStore = create<ThemeStore>(() => ({
  theme: initializeTheme(),
  setTheme: setThemePreference,
}));
