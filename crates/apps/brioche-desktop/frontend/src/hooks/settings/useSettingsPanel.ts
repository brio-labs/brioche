import { useCallback, useEffect, useMemo, useState } from "react";
import { useSettingsStore, FALLBACK_SECTIONS } from "../../stores/settingsStore";
import { setSettings, isTauri } from "../../ipc";
import type { SettingsField, SettingsSection } from "../../ipc";
import { getFieldValue } from "../../components/SettingsPanel/settingsUtils";

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

  const endpoints = useMemo(() => {
    const value = getFieldValue(settings, "memory.endpoints");
    if (Array.isArray(value)) {
      return value as Record<string, unknown>[];
    }
    return [];
  }, [settings]);

  const activeSections = useMemo(() => {
    const base = sections.length > 0 ? sections : FALLBACK_SECTIONS;
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
      await setSettings(settings);
      onClose();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setSaveError(message);
    }
  }, [settings, onClose]);

  const handleFieldChange = useCallback(
    (key: string, value: unknown) => {
      if (saveError) setSaveError(null);
      updateSetting(key, value);
    },
    [updateSetting, saveError],
  );

  const handleReset = useCallback(
    (field: SettingsField) => {
      updateSetting(field.key, field.default_value);
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
    settings,
  };
}
