export function getPathValue(obj: unknown, path: string): unknown {
    const parts = path.split(".");
    let current: unknown = obj;
    for (const part of parts) {
        if (current && typeof current === "object" && !Array.isArray(current)) {
            current = (current as Record<string, unknown>)[part];
        } else {
            return undefined;
        }
    }
    return current;
}

export function setPathValue<T extends Record<string, unknown>>(
    obj: T,
    path: string,
    value: unknown,
): T {
    const parts = path.split(".");
    if (parts.length < 2) return obj;

    const next = { ...obj };
    const moduleName = parts[0];
    const moduleObj = { ...((next[moduleName] as Record<string, unknown>) || {}) };
    next[moduleName] = moduleObj;

    let current: Record<string, unknown> = moduleObj;
    for (let i = 1; i < parts.length - 1; i++) {
        const part = parts[i];
        current[part] = { ...((current[part] as Record<string, unknown>) || {}) };
        current = current[part] as Record<string, unknown>;
    }
    current[parts[parts.length - 1]] = value;

    return next as T;
}
