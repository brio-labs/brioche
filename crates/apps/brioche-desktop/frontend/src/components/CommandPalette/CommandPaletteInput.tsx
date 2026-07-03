import { ModalSearchHeader } from "../PanelOverlay";

interface CommandPaletteInputProps {
	query: string;
	onChange: (value: string) => void;
	onKeyDown: (e: React.KeyboardEvent) => void;
	inputRef: React.RefObject<HTMLInputElement | null>;
}

export default function CommandPaletteInput({
	query,
	onChange,
	onKeyDown,
	inputRef,
}: CommandPaletteInputProps) {
	return (
		<ModalSearchHeader
			query={query}
			onChange={onChange}
			onKeyDown={onKeyDown}
			inputRef={inputRef}
			placeholder="Type a command or search..."
		/>
	);
}
