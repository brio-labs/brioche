import { create } from 'zustand';
import { type ChatMessagePayload } from './ipc';

/// Every role that a chat message can assume in the desktop UI.
///
/// Refs: I-Ui-MessageRole
export type MessageRole = 'user' | 'assistant' | 'system' | 'error' | 'tool_request' | 'tool_argument' | 'tool_done' | 'tool_result';

/// A chat message as it is stored and rendered by the frontend.
///
/// Refs: I-Ui-ChatMessage
export interface ChatMessage {
    role: MessageRole;
    content: string;
    id: string;
    toolId?: string;
    toolName?: string;
    toolArguments?: string;
    toolOutput?: string;
}

/// Optional tool-related fields that can be attached to a chat message.
///
/// Refs: I-Ui-ChatToolFields
interface ChatToolFields {
    toolId?: string;
    toolName?: string;
    toolArguments?: string;
    toolOutput?: string;
}

/// State and actions for the chat message stream.
///
/// Refs: I-Ui-ChatStore
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
    receiveMessage: (payload: ChatMessagePayload) => void;
    setMessagesFromHistory: (history: ChatMessagePayload[]) => void;
}

let messageId = 0;

/// Zustand store that owns the chat message list, input state, and streaming id.
///
/// Refs: I-Ui-ChatStore
export const useChatStore = create<ChatStore>((set, get) => ({
    messages: [],
    input: '',
    isLoading: false,
    streamingId: null,

    addMessage: (role: MessageRole, content: string, tool?: ChatToolFields) =>
        set((state) => ({
            messages: [...state.messages, { role, content, id: String(++messageId), ...tool }],
        })),

    appendMessage: (role: MessageRole, content: string, tool?: ChatToolFields) =>
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

    setInput: (input: string) => set({ input }),
    setLoading: (isLoading: boolean) => set({ isLoading }),
    clearMessages: () => set({ messages: [], streamingId: null }),

    receiveMessage: (payload: ChatMessagePayload) => {
        const role = payload.role as MessageRole;
        const tool = {
            toolId: payload.tool_id,
            toolName: payload.tool_name,
            toolArguments: payload.tool_arguments,
            toolOutput: payload.tool_output,
        };
        if (role === 'assistant') {
            get().appendMessage(role, payload.content, tool);
        } else {
            get().addMessage(role, payload.content, tool);
        }
    },

    setMessagesFromHistory: (history: ChatMessagePayload[]) => {
        const messages = history.map((msg) => ({
            role: msg.role as MessageRole,
            content: msg.content,
            id: String(++messageId),
            toolId: msg.tool_id,
            toolName: msg.tool_name,
            toolArguments: msg.tool_arguments,
            toolOutput: msg.tool_output,
        }));
        set({ messages, streamingId: null });
    },
}));
