import { Select, SelectContent, SelectItem, SelectTrigger } from "../ui";

/// Props for a select that can represent true, false, or null.
///
/// Refs: I-Shell-Runtime-OnlyIO
export interface NullableBooleanSelectProps {
	value: boolean | null;
	placeholder: string;
	onChange: (value: boolean | null) => void;
}

/// Renders a select dropdown with true, false, and unset (null) options.
///
/// Refs: I-Shell-Runtime-OnlyIO
export function NullableBooleanSelect({
	value,
	placeholder,
	onChange,
}: NullableBooleanSelectProps) {
	return (
		<Select
			value={value === null ? "unset" : String(value)}
			onValueChange={(v) => onChange(v === "unset" ? null : v === "true")}
		>
			<SelectTrigger className="flex-1 px-2.5 py-1.5 text-xs" />
			<SelectContent>
				<SelectItem value="unset">{placeholder}</SelectItem>
				<SelectItem value="true">enabled</SelectItem>
				<SelectItem value="false">disabled</SelectItem>
			</SelectContent>
		</Select>
	);
}
