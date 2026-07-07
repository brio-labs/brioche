import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Event } from "@tauri-apps/api/event";
import { useChatStore } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { getMessages } from "../ipc";
import { isTauri } from "../ipc";
import type { ChatMessagePayload } from "../ipc";

/// Subscribes to a named Tauri event and invokes the callback on every payload.
/// Keeps the callback in a ref so the listener is not recreated on each render.
///
/// Refs: I-Ui-TauriEventBinding
export function useTauriEvent<T>(
	eventName: string,
	callback: (event: Event<T>) => void,
) {
	const callbackRef = useRef(callback);
	callbackRef.current = callback;

	useEffect(() => {
		if (!isTauri()) return;

		let unlisten: (() => void) | undefined;
		let cancelled = false;

		listen<T>(eventName, (event) => {
			if (!cancelled) {
				callbackRef.current(event);
			}
		}).then((fn) => {
			if (cancelled) {
				fn();
			} else {
				unlisten = fn;
			}
		});

		return () => {
			cancelled = true;
			if (unlisten) {
				unlisten();
			}
		};
	}, [eventName]);
}

/// Binds Tauri runtime events to the frontend Zustand stores.
///
/// Refs: I-Ui-TauriStateSync
export function useTauriSync() {
	const { receiveMessage, clearMessages, setMessagesFromHistory } = useChatStore();
	const { loadSessions } = useSessionStore();

	// Route incoming assistant/user messages into the chat store.
	useTauriEvent<ChatMessagePayload>("chat-message", (event) => {
		receiveMessage(event.payload);
	});

	// Close the window when the backend requests a clean application exit.
	useTauriEvent("app-exit", () => {
		window.close();
	});

	// Replace the chat history whenever the active session changes.
	useTauriEvent("session-changed", async () => {
		clearMessages();
		void loadSessions();

		try {
			const history = await getMessages();
			setMessagesFromHistory(history);
		} catch (err: unknown) {
			console.error("Failed to sync session history:", err);
		}
	});

	// Keep the session list in sync with the backend.
	useTauriEvent("sessions-updated", () => {
		void loadSessions();
	});
}
