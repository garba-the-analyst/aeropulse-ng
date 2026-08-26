/**
 * Tauri event bindings with an air-gapped-friendly identity check:
 * under plain `vite` dev (no Rust host) consumers fall back to the
 * synthetic demo feed instead of crashing on missing internals.
 */

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type Unlisten = () => void;

export async function listenSafe<T>(
  event: string,
  handler: (payload: T) => void,
): Promise<Unlisten> {
  if (!isTauri()) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}
