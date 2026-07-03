import { useCallback, useEffect, useRef } from "react";
import { useChatStore, type ChatMessage } from "../../store";
import { open } from "@tauri-apps/plugin-dialog";
import { sendMessage, attachReference, sendImage } from "../../ipc";

export interface ChatActions {
  messages: ChatMessage[];
  input: string;
  isLoading: boolean;
  setInput: (input: string) => void;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  handleSubmit: (e?: React.FormEvent) => Promise<void>;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void>;
  handleImage: () => Promise<void>;
  handleClearChat: () => void;
  handleExportChat: () => void;
}

export function useChatActions(): ChatActions {
  const {
    messages,
    input,
    isLoading,
    addMessage,
    setInput,
    setLoading,
    clearMessages,
  } = useChatStore();
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = useCallback(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  useEffect(() => {
    scrollToBottom();
  }, [messages, scrollToBottom]);

  const handleClearChat = useCallback(() => {
    clearMessages();
    void sendMessage("/clear");
  }, [clearMessages]);

  const handleExportChat = useCallback(() => {
    const text = messages.map((m) => `${m.role}: ${m.content}`).join("\n\n");
    const blob = new Blob([text], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `brioche-chat-${new Date().toISOString().slice(0, 10)}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }, [messages]);

  const handleSubmit = useCallback(
    async (e?: React.FormEvent) => {
      e?.preventDefault();
      const trimmed = input.trim();
      if (!trimmed || isLoading) return;

      setInput("");
      addMessage("user", trimmed);
      setLoading(true);

      try {
        await sendMessage(trimmed);
      } catch (err) {
        addMessage("error", String(err));
      } finally {
        setLoading(false);
      }
    },
    [input, isLoading, addMessage, setInput, setLoading],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        void handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleAttach = useCallback(async () => {
    const path = await open({
      multiple: false,
      directory: false,
    });
    if (!path) return;
    try {
      await attachReference(path);
      addMessage("system", `Attached: ${path}`);
    } catch (err) {
      addMessage("error", String(err));
    }
  }, [addMessage]);

  const handleImage = useCallback(async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [
        { name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] },
      ],
    });
    if (!path) return;
    try {
      const dataUrl = await sendImage(path);
      addMessage("user", `![${path}](${dataUrl})`);
    } catch (err) {
      addMessage("error", String(err));
    }
  }, [addMessage]);

  return {
    messages,
    input,
    isLoading,
    setInput,
    messagesEndRef,
    handleSubmit,
    handleKeyDown,
    handleAttach,
    handleImage,
    handleClearChat,
    handleExportChat,
  };
}
