import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";

export function useMaximized() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    const win = getCurrentWindow();
    void win.isMaximized().then((m) => {
      if (!cancelled) setMaximized(m);
    });

    void win
      .onResized(() => {
        void win.isMaximized().then((m) => {
          if (!cancelled) setMaximized(m);
        });
      })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      });

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, []);

  return maximized;
}
