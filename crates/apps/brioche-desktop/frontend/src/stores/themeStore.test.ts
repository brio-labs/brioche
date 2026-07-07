// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import {
  initializeTheme,
  setThemePreference,
  useThemeStore,
} from "./themeStore";

const THEME_STORAGE_KEY = "brioche.theme";

describe("themeStore", () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.colorScheme = "";
    useThemeStore.setState({ theme: "brio" });
  });

  it("initializes from the persisted Catppuccin preference", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "catppuccin-mocha");

    const theme = initializeTheme();

    expect(theme).toBe("catppuccin-mocha");
    expect(document.documentElement.dataset.theme).toBe("catppuccin-mocha");
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("rejects an invalid persisted theme instead of applying it to the document", () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "solarized");

    const theme = initializeTheme();

    expect(theme).toBe("brio");
    expect(document.documentElement.dataset.theme).toBe("brio");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("solarized");
  });

  it("persists Catppuccin and updates the runtime theme surfaces", () => {
    setThemePreference("catppuccin-mocha");

    expect(document.documentElement.dataset.theme).toBe("catppuccin-mocha");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("catppuccin-mocha");
    expect(useThemeStore.getState().theme).toBe("catppuccin-mocha");
  });
});
