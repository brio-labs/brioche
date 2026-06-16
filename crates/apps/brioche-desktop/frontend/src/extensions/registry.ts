export type PanelSlot = 'left' | 'right' | 'center' | 'bottom';

export interface ExtensionMetadata {
    id: string;
    name: string;
    version: string;
    default_panel: PanelSlot | null;
    enabled: boolean;
}

export interface PanelContribution {
    id: string;
    slot: PanelSlot;
    title: string;
    component: () => Promise<{ default: React.ComponentType }>;
}

const panelContributions: PanelContribution[] = [
    {
        id: 'sessions',
        slot: 'left',
        title: 'Sessions',
        component: () => import('../components/SessionSidebar'),
    },
    {
        id: 'files',
        slot: 'right',
        title: 'Explorer',
        component: () => import('../components/FileExplorer'),
    },
];

export function registerPanel(contribution: PanelContribution) {
    const idx = panelContributions.findIndex((p) => p.id === contribution.id);
    if (idx >= 0) {
        panelContributions[idx] = contribution;
    } else {
        panelContributions.push(contribution);
    }
}

export function getPanelsForSlot(slot: PanelSlot): PanelContribution[] {
    return panelContributions.filter((p) => p.slot === slot);
}

export function getAllPanels(): PanelContribution[] {
    return [...panelContributions];
}

export function movePanel(id: string, slot: PanelSlot) {
    const panel = panelContributions.find((p) => p.id === id);
    if (panel) {
        panel.slot = slot;
    }
}
