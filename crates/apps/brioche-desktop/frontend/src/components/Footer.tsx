import { useEffect, useState } from 'react';
import { getFooterMetrics, onChatMessage } from '../ipc';
import type { FooterMetric } from '../ipc';

export default function Footer() {
    const [metrics, setMetrics] = useState<FooterMetric[]>([]);

    const load = async () => {
        try {
            const data = await getFooterMetrics();
            setMetrics(data);
        } catch (err) {
            console.error('Failed to load footer metrics:', err);
        }
    };

    useEffect(() => {
        load();
        const interval = setInterval(load, 1000);
        let unlisten: (() => void) | undefined;
        onChatMessage(() => {
            void load();
        }).then((fn) => {
            unlisten = fn;
        });
        return () => {
            clearInterval(interval);
            if (unlisten) unlisten();
        };
    }, []);

    return (
        <footer className="footer">
            {metrics.length === 0 ? (
                <div className="footer-metric">
                    <span className="footer-label">Brioche</span>
                    <span className="footer-value">0.1.0</span>
                </div>
            ) : (
                metrics.map((m) => (
                    <div key={m.id} className="footer-metric" title={m.tooltip || undefined}>
                        <span className="footer-label">{m.label}</span>
                        <span className="footer-value">{m.value}</span>
                    </div>
                ))
            )}
        </footer>
    );
}
