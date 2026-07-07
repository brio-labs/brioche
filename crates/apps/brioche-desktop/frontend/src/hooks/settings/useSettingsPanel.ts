import { useCallback, useEffect, useMemo, useState } from "react";
import { useSettingsStore } from "../../stores/settingsStore";
import {
  APPEARANCE_SETTINGS_SECTION,
  FALLBACK_SECTIONS,
} from "../../stores/settingsSections";
import { setSettings, isTauri } from "../../ipc";
import type { SettingsField, SettingsSection } from "../../ipc";
import { getFieldValue } from "../../components/SettingsPanel/settingsUtils";
import { initializeTheme, setThemePreference } from "../../stores/themeStore";

export interface UseSettingsPanelResult {
  search: string;
  setSearch: (search: string) => void;
  selectedSectionId: string | null;
  setSelectedSectionId: (id: string | null) => void;
  saveError: string | null;
  editingProtected: Set<string>;
  setEditingProtected: React.Dispatch<React.SetStateAction<Set<string>>>;
  handleSave: () => Promise<void>;
  handleFieldChange: (key: string, value: unknown) => void;
  handleReset: (field: SettingsField) => void;
  filteredSections: SettingsSection[];
  activeSections: SettingsSection[];
  selectedSection: SettingsSection | null;
  endpoints: Record<string, unknown>[];
  isTauriAvailable: boolean;
  settings: Record<string, unknown>;
}

/// Hook that manages state and derived data for the settings panel.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function useSettingsPanel(
  onClose: () => void,
): UseSettingsPanelResult {
  const { settings, loadSettings, updateSetting, sections, loadSections } =
    useSettingsStore();
  const [theme, setThemeState] = useState(initializeTheme);
  const [selectedSectionId, setSelectedSectionId] = useState<string | null>(
    null,
  );
  const [search, setSearch] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [editingProtected, setEditingProtected] = useState<Set<string>>(
    new Set(),
  );
  const isTauriAvailable = isTauri();

  useEffect(() => {
    if (!isTauriAvailable) return;
    loadSettings();
    loadSections();
  }, [loadSettings, loadSections, isTauriAvailable]);

  useEffect(() => {
    const configuredTheme = getFieldValue(settings, "ui.theme");
    if (
      (configuredTheme === "brio" || configuredTheme === "catppuccin-mocha") &&
      configuredTheme !== theme
    ) {
      setThemePreference(configuredTheme);
      setThemeState(configuredTheme);
    }
  }, [settings, theme]);

  const effectiveSettings = useMemo(() => {
    if (getFieldValue(settings, "ui.theme") !== undefined) return settings;
    return {
      ...settings,
      ui: {
        ...((settings.ui as Record<string, unknown> | undefined) ?? {}),
        theme,
      },
    };
  }, [settings, theme]);

  const endpoints = useMemo(() => {
    const value = getFieldValue(effectiveSettings, "memory.endpoints");
    if (Array.isArray(value)) {
      return value as Record<string, unknown>[];
    }
    return [];
  }, [effectiveSettings]);

  const activeSections = useMemo(() => {
    const backendSections =
      sections.length > 0
        ? sections.filter((section) => section.id !== "appearance")
        : FALLBACK_SECTIONS.filter((section) => section.id !== "appearance");
    const base = [APPEARANCE_SETTINGS_SECTION, ...backendSections];
    return base.map((section) => {
      if (section.id !== "memory-providers") return section;
      return {
        ...section,
        fields: section.fields.map((field) => {
          if (field.key !== "memory.active_providers") return field;
          const endpointOptions = endpoints
            .map((ep) => ep.id)
            .filter((id): id is string => typeof id === "string")
            .map((id) => ({ value: id, label: id }));
          return {
            ...field,
            options: [
              { value: "memory-local", label: "Local memory" },
              ...endpointOptions,
            ],
          };
        }),
      };
    });
  }, [sections, endpoints]);

  const filteredSections = useMemo(() => {
    if (!search.trim()) return activeSections;
    const q = search.toLowerCase();
    return activeSections.filter(
      (s) =>
        s.title.toLowerCase().includes(q) ||
        s.keywords.some((k) => k.toLowerCase().includes(q)),
    );
  }, [activeSections, search]);

  const selectedSection = useMemo(() => {
    if (!selectedSectionId) return null;
    return activeSections.find((s) => s.id === selectedSectionId) || null;
  }, [selectedSectionId, activeSections]);

  const handleSave = useCallback(async () => {
    setSaveError(null);
    try {
      await setSettings(effectiveSettings);
      onClose();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setSaveError(message);
    }
  }, [effectiveSettings, onClose]);

  const handleFieldChange = useCallback(
    (key: string, value: unknown) => {
      if (saveError) setSaveError(null);
      updateSetting(key, value);
      if (
        key === "ui.theme" &&
        (value === "brio" || value === "catppuccin-mocha")
      ) {
        setThemePreference(value);
        setThemeState(value);
      }
    },
    [updateSetting, saveError],
  );

  const handleReset = useCallback(
    (field: SettingsField) => {
      updateSetting(field.key, field.default_value);
      if (
        field.key === "ui.theme" &&
        (field.default_value === "brio" ||
          field.default_value === "catppuccin-mocha")
      ) {
        setThemePreference(field.default_value);
        setThemeState(field.default_value);
      }
    },
    [updateSetting],
  );

  return {
    search,
    setSearch,
    selectedSectionId,
    setSelectedSectionId,
    saveError,
    editingProtected,
    setEditingProtected,
    handleSave,
    handleFieldChange,
    handleReset,
    filteredSections,
    activeSections,
    selectedSection,
    endpoints,
    isTauriAvailable,
    settings: effectiveSettings,
  };
}
