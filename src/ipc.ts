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

/** Opens the donation page in the system browser. The URL lives in the backend only. */
export async function donateOpen(): Promise<void> {
  if (inTauri()) await invoke("donate_open");
}

/** Opens the folder with the app's log files. */
export async function logsOpen(): Promise<void> {
  if (inTauri()) await invoke("logs_open");
}

export function logUi(level: "error" | "warn" | "info", message: string): void {
  if (inTauri()) void invoke("log_ui", { level, message }).catch(() => {});
}
