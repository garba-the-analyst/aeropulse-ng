/**
 * Dual-display workspace helper.
 *
 * Display 1 (radar) auto-opens Display 2 (ops HUD) on boot via
 * openOpsHud(); this hook exposes the pairing state for UI indicators.
 */
import { useEffect, useState } from "react";
import { isTauri } from "./useTauriEvent";
import { openOpsHud } from "../lib/ipc";

export function useDualDisplay(): { hudAvailable: boolean; launch: () => void } {
  const [hudAvailable, setHudAvailable] = useState(false);

  useEffect(() => {
    if (!isTauri()) {
      // Browser dev: HUD renders as a second tab instead of a webview.
      setHudAvailable(false);
      return;
    }
    let cancelled = false;
    const check = async (): Promise<void> => {
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        const win = await WebviewWindow.getByLabel("ops_hud");
        if (!cancelled) setHudAvailable(win !== null);
      } catch {
        if (!cancelled) setHudAvailable(false);
      }
    };
    void check();
    const t = setInterval(check, 5_000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, []);

  return { hudAvailable, launch: () => void openOpsHud() };
}
