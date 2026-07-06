import Tooltip from "../Tooltip";
import { WindowControls } from "./WindowControls";
import { cn } from "../ui/lib";

export interface OverlayButtonItem {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  active: boolean;
  onClick: () => void;
}

export interface TitleBarProps {
  buttons: OverlayButtonItem[];
  projectName?: string;
}

function OverlayButton({
  label,
  icon: Icon,
  active,
  onClick,
}: OverlayButtonItem) {
  return (
    <Tooltip key={label} label={label}>
      <button
        type="button"
        onClick={onClick}
        className={cn("top-bar-button", active && "text-accent")}
        aria-pressed={active}
        aria-label={label}
      >
        <Icon className="w-5 h-5" />
      </button>
    </Tooltip>
  );
}

export function TitleBar({ buttons, projectName }: TitleBarProps) {
  const title = projectName ? `Brioche - ${projectName}` : "Brioche";

  return (
    <header className="title-bar">
      <div className="flex items-center px-3">
        <span className="font-mono text-xs text-fg-secondary">
          {title}
        </span>
      </div>
      <div className="flex-1 cursor-default" data-tauri-drag-region />
      <div className="flex items-center">
        {buttons.map((button) => (
          <OverlayButton key={button.label} {...button} />
        ))}
        <div className="w-px h-5 bg-fg-muted/30 mx-2" aria-hidden="true" />
        <WindowControls />
      </div>
    </header>
  );
}
