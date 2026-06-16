import { create } from 'zustand';

export type MessageRole = 'user' | 'assistant' | 'system' | 'error' | 'tool_request' | 'tool_argument' | 'tool_done' | 'tool_result';

export interface ChatMessage {
    role: MessageRole;
    content: string;
    id: string;
    toolId?: string;
    toolName?: string;
    toolArguments?: string;
    toolOutput?: string;
}

interface ChatToolFields {
    toolId?: string;
    toolName?: string;
    toolArguments?: string;
    toolOutput?: string;
}

interface ChatStore {
    messages: ChatMessage[];
    input: string;
    isLoading: boolean;
    streamingId: string | null;
    addMessage: (role: MessageRole, content: string, tool?: ChatToolFields) => void;
    appendMessage: (role: MessageRole, content: string, tool?: ChatToolFields) => void;
    setInput: (input: string) => void;
    setLoading: (loading: boolean) => void;
    clearMessages: () => void;
}

let messageId = 0;

export const useChatStore = create<ChatStore>((set) => ({
    messages: [],
    input: '',
    isLoading: false,
    streamingId: null,
    addMessage: (role, content, tool) =>
        set((state) => ({
            messages: [...state.messages, { role, content, id: String(++messageId), ...tool }],
        })),
    appendMessage: (role, content, tool) =>
        set((state) => {
            // If we're streaming assistant text, append to the last assistant message
            if (role === 'assistant' && state.streamingId) {
                const lastMsg = state.messages[state.messages.length - 1];
                if (lastMsg && lastMsg.id === state.streamingId && lastMsg.role === 'assistant') {
                    return {
                        messages: [
                            ...state.messages.slice(0, -1),
                            { ...lastMsg, content: lastMsg.content + content },
                        ],
                    };
                }
            }
            // Start a new streaming message
            const id = String(++messageId);
            return {
                messages: [...state.messages, { role, content, id, ...tool }],
                streamingId: role === 'assistant' ? id : state.streamingId,
            };
        }),
    setInput: (input) => set({ input }),
    setLoading: (isLoading) => set({ isLoading }),
    clearMessages: () => set({ messages: [], streamingId: null }),
}));
