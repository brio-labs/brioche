import { SearchIcon, BrainIcon, BookIcon, UserIcon, WrenchIcon, SettingsIcon } from "../Icons";
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
      icon: SearchIcon,
      active: showMessageSearch,
      onClick: () => setShowMessageSearch(true),
    },
    {
      label: "Memory",
      icon: BrainIcon,
      active: showMemory,
      onClick: () => setShowMemory(true),
    },
    {
      label: "Skills",
      icon: BookIcon,
      active: showSkills,
      onClick: () => setShowSkills(true),
    },
    {
      label: "Profiles",
      icon: UserIcon,
      active: showProfiles,
      onClick: () => setShowProfiles(true),
    },
    {
      label: "Tools",
      icon: WrenchIcon,
      active: showTools,
      onClick: () => setShowTools(true),
    },
    {
      label: "Settings",
      icon: SettingsIcon,
      active: showSettings,
      onClick: () => setShowSettings(true),
    },
  ];

  return <TitleBar buttons={overlayButtons} projectName={projectName} />;
}
