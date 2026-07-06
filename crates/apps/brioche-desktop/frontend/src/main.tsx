/// Desktop frontend entry point. Renders the root React application in strict mode.
///
/// Refs: I-Ui-Entry
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { TooltipProvider } from '@radix-ui/react-tooltip';
import App from './components/App';
import { initializeTheme } from './stores/themeStore';
import './styles/global.css';

initializeTheme();

const container: HTMLElement | null = document.getElementById('root');
if (!container) {
    throw new Error('Root element not found');
}

const root = createRoot(container);
root.render(
    <StrictMode>
        <TooltipProvider delayDuration={150}>
            <App />
        </TooltipProvider>
    </StrictMode>,
);
