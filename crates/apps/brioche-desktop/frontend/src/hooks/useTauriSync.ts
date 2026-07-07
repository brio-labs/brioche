import { useEffect, useRef } from "react";
import { listen, type Event } from "@tauri-apps/api/event";
import { useChatStore } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { getMessages, type ChatMessagePayload } from "../ipc";

/**
 * A reactive hook to listen to Tauri events with proper cleanup and callback stability.
 * Uses a ref for the callback to prevent unnecessary listener unregistration/registration.
 *
 * Refs: I-Ui-TauriEventBinding
 */
export function useTauriEvent<T>(
	eventName: string,
	callback: (event: Event<T>) => void,
) {
	const callbackRef = useRef(callback);
	callbackRef.current = callback;

	useEffect(() => {
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

/**
 * A reactive state-synchronization hook that binds Tauri window/system events
 * directly to frontend Zustand store actions.
 *
 * Refs: I-Ui-TauriStateSync
 */
export function useTauriSync() {
	const { receiveMessage, clearMessages, setMessagesFromHistory } = useChatStore();
	const { loadSessions } = useSessionStore();

	// 1. Reactive incoming message routing
	useTauriEvent<ChatMessagePayload>("chat-message", (event) => {
		receiveMessage(event.payload);
	});

	// 2. Safe application exit request
	useTauriEvent("app-exit", () => {
		window.close();
	});

	// 3. Batched history retrieval upon active session change
	useTauriEvent("session-changed", async () => {
		clearMessages();
		void loadSessions();

		try {
			const history = await getMessages();
			setMessagesFromHistory(history);
		} catch (err) {
			console.error("Failed to sync session history:", err);
		}
	});

	// 4. Session list synchronization
	useTauriEvent("sessions-updated", () => {
		void loadSessions();
	});
}
