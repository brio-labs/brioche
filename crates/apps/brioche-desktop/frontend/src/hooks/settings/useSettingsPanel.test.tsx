// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import type { Settings } from "../../ipc";
import type { UseSettingsPanelResult } from "./useSettingsPanel";

vi.mock("../../ipc", () => ({
  getSettings: vi.fn(),
  setSettings: vi.fn(),
  listSettingsSections: vi.fn(),
  isTauri: vi.fn(() => false),
}));

import { setSettings } from "../../ipc";
import { useSettingsStore } from "../../stores/settingsStore";
import { useThemeStore } from "../../stores/themeStore";
import { useSettingsPanel } from "./useSettingsPanel";

const THEME_STORAGE_KEY = "brioche.theme";
const mockedSetSettings = vi.mocked(setSettings);

function resetStores(settings: Settings = {}) {
  useSettingsStore.setState({
    settings,
    sections: [],
    isLoading: false,
    hasLoaded: false,
  });
  useThemeStore.setState({ theme: "brio" });
}

function HookHarness({
  children,
  onClose,
}: {
  children: (panel: UseSettingsPanelResult) => ReactNode;
  onClose: () => void;
}) {
  const panel = useSettingsPanel(onClose);
  return <>{children(panel)}</>;
}

function renderPanel({
  onClose = vi.fn(),
  settings = {},
}: {
  onClose?: () => void;
  settings?: Settings;
} = {}) {
  let latest: UseSettingsPanelResult | null = null;
  resetStores(settings);

  render(
    <HookHarness onClose={onClose}>
      {(panel) => {
        latest = panel;
        return null;
      }}
    </HookHarness>,
  );

  return {
    onClose,
    get panel() {
      if (!latest) throw new Error("settings panel hook did not render");
      return latest;
    },
  };
}

describe("useSettingsPanel theme integration", () => {
  beforeEach(() => {
    cleanup();
    vi.clearAllMocks();
    window.localStorage.clear();
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.style.colorScheme = "";
    resetStores();
  });

  it("includes the persisted runtime theme when saving settings without a backend theme", async () => {
    window.localStorage.setItem(THEME_STORAGE_KEY, "catppuccin-mocha");
    const onClose = vi.fn();
    mockedSetSettings.mockResolvedValue(undefined);

    const { panel } = renderPanel({
      onClose,
      settings: {
        ui: { working_dir: "/workspace" },
        chat: { model: "claude" },
      },
    });

    expect(panel.settings).toEqual({
      ui: { working_dir: "/workspace", theme: "catppuccin-mocha" },
      chat: { model: "claude" },
    });

    await act(async () => {
      await panel.handleSave();
    });

    expect(mockedSetSettings).toHaveBeenCalledWith({
      ui: { working_dir: "/workspace", theme: "catppuccin-mocha" },
      chat: { model: "claude" },
    });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("applies and persists Catppuccin immediately when the Appearance field changes", () => {
    const rendered = renderPanel({
      settings: { ui: { working_dir: "/workspace" } },
    });

    act(() => {
      rendered.panel.handleFieldChange("ui.theme", "catppuccin-mocha");
    });

    expect(rendered.panel.settings).toEqual({
      ui: { working_dir: "/workspace", theme: "catppuccin-mocha" },
    });
    expect(document.documentElement.dataset.theme).toBe("catppuccin-mocha");
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("catppuccin-mocha");
    expect(useThemeStore.getState().theme).toBe("catppuccin-mocha");
  });
});
