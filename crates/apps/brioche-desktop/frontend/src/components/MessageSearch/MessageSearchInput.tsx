import { ModalSearchHeader } from "../PanelOverlay";

export interface MessageSearchInputProps {
	query: string;
	onChange: (value: string) => void;
	onKeyDown: (e: React.KeyboardEvent) => void;
	inputRef: React.RefObject<HTMLInputElement | null>;
	placeholder: string;
	onClear: () => void;
}

/**
 * Search input header for the message search overlay.
 *
 * Refs: I-Ui-OverlayCohesion
 */
export function MessageSearchInput(props: MessageSearchInputProps) {
	return <ModalSearchHeader {...props} />;
}
