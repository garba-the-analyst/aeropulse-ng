/**
 * Thin Tauri IPC accessors with graceful browser-dev fallbacks so the UI
 * boots under plain `vite` without the Rust host present.
 */
import { isTauri } from "../hooks/useTauriEvent";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function invokeSafe<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!isTauri()) return null;
  try {
    const mod = await import("@tauri-apps/api/core");
    return (await mod.invoke<T>(cmd, args)) as T;
  } catch (err) {
    console.warn(`[ipc] ${cmd} failed`, err);
    return null;
  }
}

export async function openOpsHud(): Promise<void> {
  if (!isTauri()) {
    window.open("/hud.html", "_blank", "width=1280,height=860");
    return;
  }
  try {
    const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    const existing = await WebviewWindow.getByLabel("ops_hud");
    if (!existing) {
      void new WebviewWindow("ops_hud", {
        url: "hud.html",
        title: "AeroPulse-NG :: Operations & Weather HUD",
        width: 1280,
        height: 860,
      });
    } else {
      await existing.setFocus();
    }
  } catch (err) {
    console.warn("[ipc] ops_hud open failed", err);
    window.open("/hud.html", "_blank", "width=1280,height=860");
  }
}
