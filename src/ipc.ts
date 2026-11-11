// Typed wrappers over the backend commands. Types are kept in sync with src-tauri by hand:
// changing a Rust struct means changing its type here.

import { invoke } from "@tauri-apps/api/core";

export type Plan = "free" | "pro";

/** src-tauri/src/entitlements.rs → Entitlements */
export interface Entitlements {
  plan: Plan;
  showPromo: boolean;
  skills: boolean;
  folders: boolean;
}

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

// In a plain browser there is no backend: the layout still renders with the free plan.
const BROWSER_ENTITLEMENTS: Entitlements = { plan: "free", showPromo: true, skills: false, folders: false };

export async function entitlements(): Promise<Entitlements> {
  return inTauri() ? invoke<Entitlements>("entitlements_get") : BROWSER_ENTITLEMENTS;
}

export async function appInfo(): Promise<AppInfo | null> {
  return inTauri() ? invoke<AppInfo>("app_info") : null;
}

export function logUi(level: "error" | "warn" | "info", message: string): void {
  if (inTauri()) void invoke("log_ui", { level, message }).catch(() => {});
}
