import {
	useEffect,
	useRef,
	useCallback,
	useState,
	Suspense,
	lazy,
} from "react";
import { useChatStore, type MessageRole } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useFileStore } from "../stores/fileStore";
import { open } from "@tauri-apps/plugin-dialog";
import {
	sendMessage,
	onChatMessage,
	onAppExit,
	onSessionChanged,
	onSessionsUpdated,
	getMessages,
	attachReference,
	sendImage,
} from "../ipc";
import Footer from "./Footer";
import {
	MenuIcon,
	SettingsIcon,
	ClearIcon,
	SendIcon,
	BrainIcon,
	BookIcon,
	PaperclipIcon,
	ImageIcon,
	WrenchIcon,
} from "./Icons";
import { getPanelsForSlot, type PanelSlot } from "../extensions/registry";
import SettingsPanel from "./SettingsPanel";
import SkillsPanel from "./SkillsPanel";
import MemoryPanel from "./MemoryPanel";
import ToolCallMessage from "./ToolCallMessage";

const LazyPanel = lazy(() => import("./LazyPanel"));

interface PanelState {
	left: boolean;
	right: boolean;
	bottom: boolean;
}

export default function App() {
	const {
		messages,
		input,
		isLoading,
		addMessage,
		appendMessage,
		setInput,
		setLoading,
		clearMessages,
	} = useChatStore();
	const { loadSessions } = useSessionStore();
	const { loadSettings, settings } = useSettingsStore();
	const { loadDirectory } = useFileStore();
	const messagesEndRef = useRef<HTMLDivElement>(null);
	const [showSettings, setShowSettings] = useState(false);
	const [showSkills, setShowSkills] = useState(false);
	const [showMemory, setShowMemory] = useState(false);
	const [panels, setPanels] = useState<PanelState>({
		left: true,
		right: true,
		bottom: false,
	});

	const scrollToBottom = useCallback(() => {
		messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
	}, []);

	useEffect(() => {
		scrollToBottom();
	}, [messages, scrollToBottom]);

	useEffect(() => {
		loadSessions();
		loadSettings();
	}, [loadSessions, loadSettings]);

	useEffect(() => {
		const workingDir = (settings.ui as Record<string, unknown> | undefined)
			?.working_dir as string | undefined;
		if (workingDir) {
			loadDirectory(workingDir);
		}
	}, [settings, loadDirectory]);

	useEffect(() => {
		let unlistenChat: (() => void) | undefined;
		let unlistenExit: (() => void) | undefined;
		let unlistenSessionChanged: (() => void) | undefined;
		let unlistenSessionsUpdated: (() => void) | undefined;
		let cancelled = false;

		onChatMessage((msg) => {
			if (cancelled) return;
			const role = msg.role as MessageRole;
			const tool = {
				toolId: msg.tool_id,
				toolName: msg.tool_name,
				toolArguments: msg.tool_arguments,
				toolOutput: msg.tool_output,
			};
			if (role === "assistant") {
				appendMessage(role, msg.content, tool);
			} else {
				addMessage(role, msg.content, tool);
			}
		}).then((fn) => {
			if (cancelled) fn();
			else unlistenChat = fn;
		});

		onAppExit(() => {
			if (!cancelled) window.close();
		}).then((fn) => {
			if (cancelled) fn();
			else unlistenExit = fn;
		});

		onSessionChanged(async () => {
			if (cancelled) return;
			clearMessages();
			loadSessions();
			try {
				const history = await getMessages();
				history.forEach((msg) => {
					const role = msg.role as MessageRole;
					const tool = {
						toolId: msg.tool_id,
						toolName: msg.tool_name,
						toolArguments: msg.tool_arguments,
						toolOutput: msg.tool_output,
					};
					if (role === "assistant") {
						appendMessage(role, msg.content, tool);
					} else {
						addMessage(role, msg.content, tool);
					}
				});
			} catch (err) {
				console.error("Failed to load session messages:", err);
			}
		}).then((fn) => {
			if (cancelled) fn();
			else unlistenSessionChanged = fn;
		});

		onSessionsUpdated(() => {
			if (!cancelled) loadSessions();
		}).then((fn) => {
			if (cancelled) fn();
			else unlistenSessionsUpdated = fn;
		});

		return () => {
			cancelled = true;
			if (unlistenChat) unlistenChat();
			if (unlistenExit) unlistenExit();
			if (unlistenSessionChanged) unlistenSessionChanged();
			if (unlistenSessionsUpdated) unlistenSessionsUpdated();
		};
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []);

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

	const handleAttach = async () => {
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
	};

	const handleImage = async () => {
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
	};

	const leftPanels = getPanelsForSlot("left");
	const rightPanels = getPanelsForSlot("right");
	const bottomPanels = getPanelsForSlot("bottom");

	const renderPanel = (slot: PanelSlot) => {
		const panelsList =
			slot === "left"
				? leftPanels
				: slot === "right"
					? rightPanels
					: bottomPanels;
		return panelsList.map((p) => (
			<Suspense
				key={p.id}
				fallback={<div className="panel-loading">Loading...</div>}
			>
				<LazyPanel loader={p.component} />
			</Suspense>
		));
	};

	return (
		<div className="app">
			<div className={`panel panel-left ${panels.left ? "" : "collapsed"}`}>
				{renderPanel("left")}
			</div>

			<div className="main-panel">
				<header className="header">
					<div className="header-left">
						<button
							type="button"
							className="icon-btn"
							onClick={() => setPanels((p) => ({ ...p, left: !p.left }))}
							title="Toggle left panel"
						>
							<MenuIcon />
						</button>
						<span className="header-title">Brioche</span>
					</div>
					<div className="header-right">
						<button
							type="button"
							className="header-btn"
							onClick={() => setShowMemory(true)}
							title="Memory"
						>
							<BrainIcon />
							<span>Memory</span>
						</button>
						<button
							type="button"
							className="header-btn"
							onClick={() => setShowSkills(true)}
							title="Skills"
						>
							<BookIcon />
							<span>Skills</span>
						</button>
						<button
							type="button"
							className="header-btn"
							onClick={() => setPanels((p) => ({ ...p, bottom: !p.bottom }))}
							title="Toggle tools"
						>
							<WrenchIcon />
							<span>Tools</span>
						</button>
						<button
							type="button"
							className="header-btn"
							onClick={() => {
								clearMessages();
								void sendMessage("/clear");
							}}
							title="Clear history"
						>
							<ClearIcon />
							<span>Clear</span>
						</button>
						<button
							type="button"
							className="header-btn"
							onClick={() => setShowSettings(true)}
							title="Settings"
						>
							<SettingsIcon />
							<span>Settings</span>
						</button>
						<button
							type="button"
							className="icon-btn"
							onClick={() => setPanels((p) => ({ ...p, right: !p.right }))}
							title="Toggle right panel"
						>
							<MenuIcon />
						</button>
					</div>
				</header>

				<div className="messages">
					{messages.length === 0 && (
						<div className="empty-state">
							<div className="empty-state-title">Brioche Desktop</div>
							<div className="empty-state-hint">
								Type a message or use /help for commands
							</div>
						</div>
					)}
					{messages.map((msg) =>
						msg.role === 'tool_request' || msg.role === 'tool_result' ? (
							<div key={msg.id} className={`message ${msg.role}`}>
								<ToolCallMessage message={msg} />
							</div>
						) : (
							<div key={msg.id} className={`message ${msg.role}`}>
								<div className="message-header">
									<span className="message-role">{msg.role}</span>
								</div>
								<div className="message-body">
									<div className="message-content">{msg.content}</div>
								</div>
							</div>
						)
					)}
					{isLoading && (
						<div className="message assistant">
							<div className="message-header">
								<span className="message-role">assistant</span>
							</div>
							<div className="message-body">
								<div className="message-content loading">Thinking...</div>
							</div>
						</div>
					)}
					<div ref={messagesEndRef} />
				</div>

				<form className="input-bar" onSubmit={handleSubmit}>
					<div className="input-actions">
						<button
							type="button"
							className="icon-btn"
							onClick={handleAttach}
							title="Attach file/folder"
						>
							<PaperclipIcon />
						</button>
						<button
							type="button"
							className="icon-btn"
							onClick={handleImage}
							title="Send image"
						>
							<ImageIcon />
						</button>
					</div>
					<textarea
						value={input}
						onChange={(e) => setInput(e.target.value)}
						onKeyDown={handleKeyDown}
						placeholder="Type a message or /help..."
						disabled={isLoading}
						rows={1}
					/>
					<button type="submit" disabled={isLoading || !input.trim()}>
						<SendIcon />
					</button>
				</form>

				{panels.bottom && (
					<div className="panel panel-bottom">{renderPanel("bottom")}</div>
				)}

				<Footer />
			</div>

			<div className={`panel panel-right ${panels.right ? "" : "collapsed"}`}>
				{renderPanel("right")}
			</div>

			{showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
			{showSkills && <SkillsPanel onClose={() => setShowSkills(false)} />}
			{showMemory && <MemoryPanel onClose={() => setShowMemory(false)} />}
		</div>
	);
}
