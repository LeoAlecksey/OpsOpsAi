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

/** Opens the folder with the app's log files. */
export async function logsOpen(): Promise<void> {
  if (inTauri()) await invoke("logs_open");
}

export function logUi(level: "error" | "warn" | "info", message: string): void {
  if (inTauri()) void invoke("log_ui", { level, message }).catch(() => {});
}

// ---------- updates ----------

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

// ---------- models ----------

export type FitLevel = "good" | "slow" | "tight" | "no";

/** src-tauri/src/hw.rs → Hardware */
export interface Hardware {
  os: string;
  arch: string;
  ramTotal: number;
  diskFree: number | null;
  cpuThreads: number;
  gpu: boolean;
  gpuName: string;
}

/** src-tauri/src/models.rs → ModelItem */
export interface ModelItem {
  id: string;
  title: string;
  family: string;
  paramsB: number;
  activeB: number;
  ramGb: number;
  license: string;
  description: string;
  vision: boolean;
  size: number;
  installed: boolean;
  installing: boolean;
  fit: { level: FitLevel; why: string };
}

/** src-tauri/src/models.rs → ModelList */
export interface ModelList {
  hardware: Hardware;
  selected: string | null;
  running: string | null;
  recommended: ModelItem[];
  possible: ModelItem[];
  hidden: number;
}

/** Payload of `model-progress` events. */
export interface ModelProgress {
  id: string;
  stage: "engine" | "model" | "vision";
  done: number;
  total: number;
  check: boolean;
}

export async function modelsList(): Promise<ModelList | null> {
  return inTauri() ? invoke<ModelList>("models_list") : null;
}

export const modelInstall = (id: string) => invoke<void>("model_install", { id });
export const modelCancel = (id: string) => invoke<void>("model_cancel", { id });
export const modelRemove = (id: string) => invoke<void>("model_remove", { id });
export const modelSelect = (id: string) => invoke<void>("model_select", { id });

export interface ModelEvents {
  progress: (p: ModelProgress) => void;
  installed: (id: string) => void;
  error: (id: string, error: string) => void;
}

/** Subscribes to installation events of all models; returns the unsubscribe function. */
export async function onModelEvents(h: ModelEvents): Promise<() => void> {
  if (!inTauri()) return () => {};
  const offs = await Promise.all([
    listen<ModelProgress>("model-progress", (e) => h.progress(e.payload)),
    listen<{ id: string }>("model-installed", (e) => h.installed(e.payload.id)),
    listen<{ id: string; error: string }>("model-error", (e) => h.error(e.payload.id, e.payload.error)),
  ]);
  return () => offs.forEach((off) => off());
}

// ---------- chat ----------

/** src-tauri/src/chat.rs → Msg */
export interface ApiMsg {
  role: "system" | "user" | "assistant";
  content: string;
  images: string[];
}

/** Payload of `chat-<id>` events. */
export interface ChatEvent {
  delta?: string;
  done?: boolean;
  error?: string;
  status?: "starting" | "answering";
}

/** Sends the conversation; events arrive until {done} or {error}. */
export async function chatSend(id: string, messages: ApiMsg[], onEvent: (e: ChatEvent) => void): Promise<void> {
  const unlisten = await listen<ChatEvent>(`chat-${id}`, (e) => onEvent(e.payload));
  try {
    await invoke("chat_send", { id, messages });
  } finally {
    unlisten();
  }
}

export const chatStop = (id: string) => invoke<void>("chat_stop", { id });

// ---------- history ----------

/** src-tauri/src/history.rs → StoredMsg */
export interface StoredMsg {
  role: "user" | "assistant";
  content: string;
  images: string[];
  attachments: string[];
}

/** src-tauri/src/history.rs → Chat */
export interface Chat {
  id: string;
  title: string;
  created: number;
  updated: number;
  model: string | null;
  skill: string | null;
  messages: StoredMsg[];
}

export interface ChatMeta {
  id: string;
  title: string;
  updated: number;
}

export async function chatsList(): Promise<ChatMeta[]> {
  return inTauri() ? invoke<ChatMeta[]>("chats_list") : [];
}
export const chatLoad = (id: string) => invoke<Chat>("chat_load", { id });
export const chatSave = (chat: Chat) => invoke<void>("chat_save", { chat });
export const chatDelete = (id: string) => invoke<void>("chat_delete", { id });

// ---------- attachments ----------

/** src-tauri/src/attach.rs → Attachment */
export interface Attachment {
  name: string;
  kind: "text" | "image";
  content: string;
  files: number;
  truncated: boolean;
  problem: string | null;
}

export async function attachPick(folder: boolean): Promise<Attachment[]> {
  return inTauri() ? invoke<Attachment[]>("attach_pick", { folder }) : [];
}
