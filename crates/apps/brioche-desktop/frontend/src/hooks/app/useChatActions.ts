import { useCallback, useEffect, useRef, useState } from "react";
import { useChatStore, type ChatMessage } from "../../store";
import { open } from "@tauri-apps/plugin-dialog";
import { sendMessage, attachReference, sendImage } from "../../ipc";

export interface Attachment {
  id: string;
  path: string;
  name: string;
  type: "document" | "image";
  dataUrl?: string;
}

export interface ChatActions {
  messages: ChatMessage[];
  input: string;
  isLoading: boolean;
  setInput: (input: string) => void;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  handleSubmit: (e?: React.FormEvent) => Promise<void>;
  handleKeyDown: (e: React.KeyboardEvent) => void;
  handleAttach: () => Promise<void>;
  handleClearChat: () => void;
  handleExportChat: () => void;
  pendingAttachments: Attachment[];
  removeAttachment: (id: string) => void;
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
  const [pendingAttachments, setPendingAttachments] = useState<Attachment[]>([]);

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
      if ((!trimmed && pendingAttachments.length === 0) || isLoading) return;

      setInput("");
      const attachmentsToSend = [...pendingAttachments];
      setPendingAttachments([]);
      setLoading(true);

      try {
        let finalContent = trimmed;

        // Process attachments
        for (const att of attachmentsToSend) {
          if (att.type === "document") {
            await attachReference(att.path);
            finalContent += `\n\nAttached: ${att.path}`;
          } else if (att.type === "image") {
            finalContent += `\n\n![${att.path}](${att.dataUrl})`;
          }
        }

        addMessage("user", finalContent.trim());
        await sendMessage(finalContent.trim());
      } catch (err) {
        addMessage("error", String(err));
      } finally {
        setLoading(false);
      }
    },
    [input, isLoading, pendingAttachments, addMessage, setInput, setLoading],
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
    
    const name = path.split(/[/\\]/).pop() || "";
    const extension = name.split(".").pop()?.toLowerCase() || "";
    const isImage = ["png", "jpg", "jpeg", "gif", "webp", "svg"].includes(extension);

    if (isImage) {
      try {
        const dataUrl = await sendImage(path);
        setPendingAttachments((prev) => [
          ...prev,
          {
            id: Math.random().toString(),
            path,
            name,
            type: "image",
            dataUrl,
          },
        ]);
      } catch (err) {
        addMessage("error", String(err));
      }
    } else {
      setPendingAttachments((prev) => [
        ...prev,
        {
          id: Math.random().toString(),
          path,
          name,
          type: "document",
        },
      ]);
    }
  }, [addMessage]);

  const removeAttachment = useCallback((id: string) => {
    setPendingAttachments((prev) => prev.filter((a) => a.id !== id));
  }, []);

  return {
    messages,
    input,
    isLoading,
    setInput,
    messagesEndRef,
    handleSubmit,
    handleKeyDown,
    handleAttach,
    handleClearChat,
    handleExportChat,
    pendingAttachments,
    removeAttachment,
  };
}
