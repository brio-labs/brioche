import {
	useEffect,
	useRef,
	useCallback,
	useState,
} from "react";
import { useChatStore } from "../store";
import { useSessionStore } from "../stores/sessionStore";
import { useSettingsStore } from "../stores/settingsStore";
import { useFileStore } from "../stores/fileStore";
import { open } from "@tauri-apps/plugin-dialog";
import {
	sendMessage,
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
	UserIcon,
} from "./Icons";
import SessionSidebar from "./SessionSidebar";
import FileExplorer from "./FileExplorer";
import ToolsPanel from "./ToolsPanel";
import SettingsPanel from "./SettingsPanel";
import SkillsPanel from "./SkillsPanel";
import MemoryPanel from "./MemoryPanel";
import ProfilesPanel from "./ProfilesPanel";
import ToolCallMessage from "./ToolCallMessage";
import { useTauriSync } from "../hooks/useTauriSync";

interface PanelState {
	left: boolean;
	right: boolean;
}

export default function App() {
	const {
		messages,
		input,
		isLoading,
		addMessage,
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
	const [showProfiles, setShowProfiles] = useState(false);
	const [showMemory, setShowMemory] = useState(false);
	const [showTools, setShowTools] = useState(false);
	const [panels, setPanels] = useState<PanelState>({
		left: true,
		right: true,
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

	// Synchronize Tauri events with stores reactively
	useTauriSync();

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


	return (
		<div className="app flex flex-row h-screen w-screen overflow-hidden relative text-text-primary">
			<div className={`flex flex-col bg-bg-1/85 backdrop-blur-md border-r border-border overflow-hidden transition-all duration-300 ease-out z-[1] max-[900px]:absolute max-[900px]:top-0 max-[900px]:bottom-0 max-[900px]:z-10 max-[900px]:left-0 ${panels.left ? "w-[280px] min-w-[280px] opacity-100" : "w-0 min-w-0 opacity-0 border-r-0 pointer-events-none"}`}>
				<SessionSidebar />
			</div>

			<div className="flex-1 flex flex-col min-w-0 overflow-hidden bg-transparent relative z-[1]">
				<header className="header flex items-center justify-between px-4 h-[52px] bg-bg-1/70 backdrop-blur-md border-b border-border shrink-0 relative">
					<div className="flex items-center gap-2">
						<button
							type="button"
							className="w-8 h-8 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
							onClick={() => setPanels((p) => ({ ...p, left: !p.left }))}
							title="Toggle left panel"
						>
							<MenuIcon className="w-4 h-4" />
						</button>
						<span className="text-sm font-semibold text-text-secondary tracking-wider">Brioche</span>
					</div>
					<div className="flex items-center gap-2">
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => setShowMemory(true)}
							title="Memory"
						>
							<BrainIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Memory</span>
						</button>
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => setShowSkills(true)}
							title="Skills"
						>
							<BookIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Skills</span>
						</button>
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => setShowProfiles(true)}
							title="Profiles"
						>
							<UserIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Profiles</span>
						</button>
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => setShowTools(true)}
							title="Toggle tools"
						>
							<WrenchIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Tools</span>
						</button>
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => {
								clearMessages();
								void sendMessage("/clear");
							}}
							title="Clear history"
						>
							<ClearIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Clear</span>
						</button>
						<button
							type="button"
							className="flex items-center gap-2 px-3 py-2 bg-transparent hover:bg-bg-3 text-text-muted hover:text-text-secondary rounded text-[11px] font-medium tracking-wider transition-all duration-200 cursor-pointer"
							onClick={() => setShowSettings(true)}
							title="Settings"
						>
							<SettingsIcon className="w-4 h-4" />
							<span className="hidden lg:inline">Settings</span>
						</button>
						<button
							type="button"
							className="w-8 h-8 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
							onClick={() => setPanels((p) => ({ ...p, right: !p.right }))}
							title="Toggle right panel"
						>
							<MenuIcon className="w-4 h-4" />
						</button>
					</div>
				</header>

				<div className="flex-1 overflow-y-auto px-5 py-4 flex flex-col gap-4 relative">
					{messages.length === 0 && (
						<div className="text-center text-text-muted mt-8 flex flex-col gap-3 items-center">
							<div className="text-base font-semibold text-text-tertiary tracking-wide">Brioche Desktop</div>
							<div className="text-[13px] text-text-muted">
								Type a message or use /help for commands
							</div>
						</div>
					)}
					{messages.map((msg) =>
						msg.role === "tool_request" || msg.role === "tool_result" ? (
							<div key={msg.id} className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
								msg.role === "tool_request" ? "self-end" : "self-start"
							}`}>
								<ToolCallMessage message={msg} />
							</div>
						) : (
							<div key={msg.id} className={`flex flex-col gap-2 relative animate-fadeIn max-w-[85%] ${
								msg.role === "user"
									? "self-end"
									: msg.role === "assistant"
										? "self-start max-w-[90%]"
										: "self-center max-w-[600px] w-full"
							}`}>
								<div className="flex items-center gap-2 mb-0.5 px-1">
									<span className="text-[10px] font-bold uppercase tracking-wider text-text-muted">{msg.role}</span>
								</div>
								<div className={`px-4 py-3 rounded-lg leading-relaxed text-sm break-words relative overflow-hidden ${
									msg.role === "user"
										? "bg-user-bg text-text-primary border border-accent/15 shadow-md"
										: msg.role === "assistant"
											? "bg-assistant-bg text-text-primary border border-border shadow-md"
											: msg.role === "system"
												? "bg-system-bg text-text-secondary border border-border rounded-lg text-xs font-mono"
												: "bg-error-bg text-[#e8a0a0] border border-error-border rounded-lg text-[13px]"
								}`}>
									<div className="message-content">{msg.content}</div>
								</div>
							</div>
						),
					)}
					{isLoading && (
						<div className="flex flex-col gap-2 relative animate-fadeIn max-w-[85%] self-start max-w-[90%]">
							<div className="flex items-center gap-2 mb-0.5 px-1">
								<span className="text-[10px] font-bold uppercase tracking-wider text-text-muted">assistant</span>
							</div>
							<div className="px-4 py-3 rounded-lg leading-relaxed text-sm break-words relative overflow-hidden bg-assistant-bg text-text-primary border border-border shadow-md">
								<div className="text-text-muted italic">Thinking...</div>
							</div>
						</div>
					)}
					<div ref={messagesEndRef} />
				</div>

				<form className="input-bar flex gap-3 px-4 py-3 bg-bg-1/80 backdrop-blur-md border-t border-border shrink-0 relative" onSubmit={handleSubmit}>
					<div className="flex items-center gap-2">
						<button
							type="button"
							className="w-8 h-8 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
							onClick={handleAttach}
							title="Attach file/folder"
						>
							<PaperclipIcon className="w-4 h-4" />
						</button>
						<button
							type="button"
							className="w-8 h-8 flex items-center justify-center rounded bg-transparent text-text-muted hover:text-text-secondary hover:bg-bg-3 active:bg-bg-4 transition-all duration-200 cursor-pointer"
							onClick={handleImage}
							title="Send image"
						>
							<ImageIcon className="w-4 h-4" />
						</button>
					</div>
					<textarea
						value={input}
						onChange={(e) => setInput(e.target.value)}
						onKeyDown={handleKeyDown}
						placeholder="Type a message or /help..."
						disabled={isLoading}
						className="flex-1 bg-bg-2 border border-border text-text-primary px-4 py-3 rounded-lg text-sm outline-none resize-none min-h-[44px] max-h-[200px] leading-relaxed transition-all duration-200 placeholder:text-text-dim disabled:opacity-50 disabled:cursor-not-allowed focus:border-accent-dim focus:bg-bg-3 focus:ring-2 focus:ring-accent-glow"
						rows={1}
					/>
					<button
						type="submit"
						className="px-6 py-3 bg-accent text-white rounded-lg cursor-pointer font-semibold text-[13px] tracking-wide transition-all duration-200 flex items-center justify-center relative overflow-hidden disabled:opacity-40 disabled:cursor-not-allowed disabled:bg-bg-5 hover:bg-accent-hover hover:shadow-lg hover:shadow-accent-glow/20 hover:-translate-y-0.5 active:translate-y-0"
						disabled={isLoading || !input.trim()}
						aria-label="Send message"
					>
						<SendIcon className="w-4 h-4" />
					</button>
				</form>

				<Footer />
			</div>

			<div className={`flex flex-col bg-bg-1/85 backdrop-blur-md border-l border-border overflow-hidden transition-all duration-300 ease-out z-[1] max-[900px]:absolute max-[900px]:top-0 max-[900px]:bottom-0 max-[900px]:z-10 max-[900px]:right-0 ${panels.right ? "w-[280px] min-w-[280px] opacity-100" : "w-0 min-w-0 opacity-0 border-l-0 pointer-events-none"}`}>
				<FileExplorer />
			</div>

			{showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
			{showSkills && <SkillsPanel onClose={() => setShowSkills(false)} />}
			{showProfiles && <ProfilesPanel onClose={() => setShowProfiles(false)} />}
			{showMemory && <MemoryPanel onClose={() => setShowMemory(false)} />}
			{showTools && <ToolsPanel onClose={() => setShowTools(false)} />}
		</div>
	);
}
