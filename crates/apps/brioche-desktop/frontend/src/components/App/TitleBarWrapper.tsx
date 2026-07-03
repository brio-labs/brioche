import { Search, Brain, BookOpen, User, Wrench, Settings } from "lucide-react";
import { TitleBar } from "../TitleBar";

interface TitleBarWrapperProps {
  projectName?: string;
  showMessageSearch: boolean;
  setShowMessageSearch: (value: boolean) => void;
  showMemory: boolean;
  setShowMemory: (value: boolean) => void;
  showSkills: boolean;
  setShowSkills: (value: boolean) => void;
  showProfiles: boolean;
  setShowProfiles: (value: boolean) => void;
  showTools: boolean;
  setShowTools: (value: boolean) => void;
  showSettings: boolean;
  setShowSettings: (value: boolean) => void;
}

export default function TitleBarWrapper({
  projectName,
  showMessageSearch,
  setShowMessageSearch,
  showMemory,
  setShowMemory,
  showSkills,
  setShowSkills,
  showProfiles,
  setShowProfiles,
  showTools,
  setShowTools,
  showSettings,
  setShowSettings,
}: TitleBarWrapperProps) {
  const overlayButtons = [
    {
      label: "Search messages (Ctrl+Shift+F)",
      icon: Search,
      active: showMessageSearch,
      onClick: () => setShowMessageSearch(true),
    },
    {
      label: "Memory",
      icon: Brain,
      active: showMemory,
      onClick: () => setShowMemory(true),
    },
    {
      label: "Skills",
      icon: BookOpen,
      active: showSkills,
      onClick: () => setShowSkills(true),
    },
    {
      label: "Profiles",
      icon: User,
      active: showProfiles,
      onClick: () => setShowProfiles(true),
    },
    {
      label: "Tools",
      icon: Wrench,
      active: showTools,
      onClick: () => setShowTools(true),
    },
    {
      label: "Settings",
      icon: Settings,
      active: showSettings,
      onClick: () => setShowSettings(true),
    },
  ];

  return <TitleBar buttons={overlayButtons} projectName={projectName} />;
}
