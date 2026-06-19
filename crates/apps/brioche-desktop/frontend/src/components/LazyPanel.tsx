import React from 'react';

interface LazyPanelProps {
    loader: () => Promise<{ default: React.ComponentType<any> }>;
    [key: string]: any;
}

export default function LazyPanel({ loader, ...rest }: LazyPanelProps) {
    const [Component, setComponent] = React.useState<React.ComponentType<any> | null>(null);

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
        return <div className="text-center text-text-muted py-4 text-xs">Loading...</div>;
    }

    return <Component {...rest} />;
}
