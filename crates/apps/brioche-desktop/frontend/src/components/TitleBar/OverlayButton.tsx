import Tooltip from "../Tooltip";
import { cn } from "../ui/lib";

export interface OverlayButtonProps {
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  active: boolean;
  onClick: () => void;
}

export function OverlayButton({
  label,
  icon: Icon,
  active,
  onClick,
}: OverlayButtonProps) {
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
