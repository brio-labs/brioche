import {
  File,
  FileArchive,
  FileAudio,
  FileCode,
  FileSpreadsheet,
  FileText,
  FileVideo,
} from "lucide-react";

interface FileTypeIconProps {
  extension: string;
  className: string;
}

export default function FileTypeIcon({ extension, className }: FileTypeIconProps) {
  switch (extension) {
    case "pdf":
      return <FileText className={`${className} text-red-400`} />;
    case "xlsx":
    case "xls":
    case "csv":
    case "ods":
      return <FileSpreadsheet className={`${className} text-emerald-400`} />;
    case "zip":
    case "tar":
    case "gz":
    case "rar":
    case "7z":
      return <FileArchive className={`${className} text-amber-400`} />;
    case "mp3":
    case "wav":
    case "ogg":
    case "aac":
      return <FileAudio className={`${className} text-sky-400`} />;
    case "mp4":
    case "mkv":
    case "avi":
    case "mov":
      return <FileVideo className={`${className} text-indigo-400`} />;
    case "js":
    case "ts":
    case "tsx":
    case "jsx":
    case "html":
    case "css":
    case "py":
    case "rs":
    case "go":
    case "json":
      return <FileCode className={`${className} text-violet-400`} />;
    default:
      return <File className={`${className} text-fg-muted`} />;
  }
}
