// Typed wrappers over the backend commands. Types are kept in sync with src-tauri by hand:
// changing a Rust struct means changing its type here.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

/** src-tauri/src/updater.rs → UpdateInfo */
export interface UpdateInfo {
  current: string;
  available: boolean;
  version: string | null;
  notes: string | null;
}

/** Payload of `update-progress` events. */
export interface UpdateProgress {
  downloaded?: number;
  total?: number | null;
  installing?: boolean;
}

export async function updateCheck(): Promise<UpdateInfo | null> {
  return inTauri() ? invoke<UpdateInfo>("update_check") : null;
}

/** Downloads, installs and restarts the app; resolves only if something went wrong before restart. */
export async function updateInstall(onProgress: (p: UpdateProgress) => void): Promise<void> {
  if (!inTauri()) return;
  const unlisten: UnlistenFn = await listen<UpdateProgress>("update-progress", (e) => onProgress(e.payload));
  try {
    await invoke("update_install");
  } finally {
    unlisten();
  }
}

/** Opens the folder with the app's log files. */
export async function logsOpen(): Promise<void> {
  if (inTauri()) await invoke("logs_open");
}

export function logUi(level: "error" | "warn" | "info", message: string): void {
  if (inTauri()) void invoke("log_ui", { level, message }).catch(() => {});
}
