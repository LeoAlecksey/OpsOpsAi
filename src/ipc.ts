// Typed wrappers over the backend commands. Types are kept in sync with src-tauri by hand:
// changing a Rust struct means changing its type here.

import { invoke } from "@tauri-apps/api/core";

/** src-tauri/src/lib.rs → AppInfo */
export interface AppInfo {
  name: string;
  version: string;
  os: string;
  arch: string;
}

/** Running inside the Tauri window (not in a plain browser via `npm run dev`). */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function appInfo(): Promise<AppInfo | null> {
  return inTauri() ? invoke<AppInfo>("app_info") : null;
}

export function logUi(level: "error" | "warn" | "info", message: string): void {
  if (inTauri()) void invoke("log_ui", { level, message }).catch(() => {});
}
