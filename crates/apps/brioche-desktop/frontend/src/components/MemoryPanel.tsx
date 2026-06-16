import { useState, useEffect, useCallback } from "react";
import {
  listMemories,
  setMemory,
  deleteMemory,
  searchMemories,
  type MemoryEntry,
} from "../ipc";

interface MemoryPanelProps {
  onClose: () => void;
}

export default function MemoryPanel({ onClose }: MemoryPanelProps) {
  const [memories, setMemories] = useState<MemoryEntry[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string>("all");
  const [isAdding, setIsAdding] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [newCategory, setNewCategory] = useState("preference");
  const [error, setError] = useState<string | null>(null);
  const [isTauriAvailable, setIsTauriAvailable] = useState(true);

  const categories = ["all", "preference", "fact", "habit", "project", "other"];

  useEffect(() => {
    // Detect whether we are running inside Tauri. The `window.__TAURI_INTERNALS__`
    // object is injected by the Tauri runtime; in a plain browser preview it is
    // undefined and all IPC calls would fail.
    setIsTauriAvailable(
      typeof window !== "undefined" &&
        typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !==
          "undefined",
    );
  }, []);

  const loadMemories = useCallback(async () => {
    try {
      setError(null);
      if (!isTauriAvailable) {
        setMemories([]);
        return;
      }
      const data =
        selectedCategory === "all"
          ? await listMemories()
          : await listMemories(selectedCategory);
      setMemories(data);
    } catch (e) {
      setError(String(e));
    }
  }, [selectedCategory, isTauriAvailable]);

  useEffect(() => {
    loadMemories();
  }, [loadMemories]);

  const handleSearch = async () => {
    if (!searchQuery.trim()) {
      loadMemories();
      return;
    }
    if (!isTauriAvailable) {
      setError("Memory search requires the Tauri desktop runtime.");
      return;
    }
    try {
      setError(null);
      const results = await searchMemories(searchQuery);
      setMemories(results);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleAdd = async () => {
    if (!newKey.trim() || !newValue.trim()) return;
    if (!isTauriAvailable) {
      setError("Adding memories requires the Tauri desktop runtime.");
      return;
    }
    try {
      setError(null);
      await setMemory(newKey.trim(), newValue.trim(), newCategory);
      setNewKey("");
      setNewValue("");
      setNewCategory("preference");
      setIsAdding(false);
      loadMemories();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleDelete = async (key: string) => {
    if (!isTauriAvailable) {
      setError("Deleting memories requires the Tauri desktop runtime.");
      return;
    }
    try {
      setError(null);
      await deleteMemory(key);
      loadMemories();
    } catch (e) {
      setError(String(e));
    }
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleDateString();
  };

  return (
    <div className="memory-panel" onClick={onClose}>
      <div className="memory-panel-content" onClick={(e) => e.stopPropagation()}>
        <div className="memory-panel-header">
          <h2>Memory</h2>
          <button className="memory-close-btn" onClick={onClose}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
              <path d="M4 4l8 8M4 12l8-8" stroke="currentColor" strokeWidth="1.5" />
            </svg>
          </button>
        </div>

        <div className="memory-search">
          <input
            type="text"
            placeholder="Search memories..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSearch()}
          />
          <button onClick={handleSearch}>Search</button>
        </div>

        <div className="memory-categories">
          {categories.map((cat) => (
            <button
              key={cat}
              className={selectedCategory === cat ? "active" : ""}
              onClick={() => setSelectedCategory(cat)}
            >
              {cat.charAt(0).toUpperCase() + cat.slice(1)}
            </button>
          ))}
        </div>

        {!isTauriAvailable && (
          <div className="memory-error">
            Memory panel preview mode: changes require the Tauri desktop app.
          </div>
        )}
        {error && <div className="memory-error">{error}</div>}

        <div className="memory-list">
          {memories.length === 0 ? (
            <div className="memory-empty">No memories yet</div>
          ) : (
            memories.map((memory) => (
              <div key={memory.key} className="memory-item">
                <div className="memory-item-header">
                  <span className="memory-key">{memory.key}</span>
                  <span className="memory-category">{memory.category}</span>
                  <button
                    className="memory-delete-btn"
                    onClick={() => handleDelete(memory.key)}
                  >
                    <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
                      <path d="M3 3l6 6M3 9l6-6" stroke="currentColor" strokeWidth="1.5" />
                    </svg>
                  </button>
                </div>
                <div className="memory-value">{memory.value}</div>
                <div className="memory-meta">
                  Updated: {formatDate(memory.updated_at)} | Accessed: {memory.access_count} times
                </div>
              </div>
            ))
          )}
        </div>

        {isAdding ? (
          <div className="memory-add-form">
            <input
              type="text"
              placeholder="Key (e.g., user_name)"
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
            />
            <textarea
              placeholder="Value"
              value={newValue}
              onChange={(e) => setNewValue(e.target.value)}
              rows={3}
            />
            <select
              value={newCategory}
              onChange={(e) => setNewCategory(e.target.value)}
            >
              {categories
                .filter((c) => c !== "all")
                .map((cat) => (
                  <option key={cat} value={cat}>
                    {cat.charAt(0).toUpperCase() + cat.slice(1)}
                  </option>
                ))}
            </select>
            <div className="memory-add-actions">
              <button onClick={handleAdd}>Save</button>
              <button onClick={() => setIsAdding(false)}>Cancel</button>
            </div>
          </div>
        ) : (
          <button className="memory-add-btn" onClick={() => setIsAdding(true)}>
            + Add Memory
          </button>
        )}
      </div>
    </div>
  );
}
