import React from 'react';

interface LazyPanelProps {
    loader: () => Promise<{ default: React.ComponentType }>;
}

export default function LazyPanel({ loader }: LazyPanelProps) {
    const [Component, setComponent] = React.useState<React.ComponentType | null>(null);

    React.useEffect(() => {
        let cancelled = false;
        loader().then((mod) => {
            if (!cancelled) setComponent(() => mod.default);
        });
        return () => {
            cancelled = true;
        };
    }, [loader]);

    if (!Component) {
        return <div className="panel-loading">Loading...</div>;
    }

    return <Component />;
}
