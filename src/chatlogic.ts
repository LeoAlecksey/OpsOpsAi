// Pure chat logic: skills, what goes to the model, titles, sizes and progress lines.
// Covered by tests/unit; the DOM code only applies these decisions.

import type { ApiMsg, Attachment, FitLevel, ModelProgress, StoredMsg } from "./ipc";

/** A skill: saved instructions for a kind of task. */
export interface Skill {
  id: string;
  title: string;
  prompt: string;
}

const BASE = "Отвечай на языке пользователя. Пиши ясно и по делу, без лишних вступлений. Если не знаешь ответа или не уверен — так и скажи.";

export const SKILLS: Skill[] = [
  { id: "chat", title: "Обычный чат", prompt: `Ты — полезный ассистент. ${BASE}` },
  {
    id: "translator",
    title: "Переводчик",
    prompt: "Ты — переводчик. Переводи текст пользователя: с русского на английский, с любого другого языка — на русский, если пользователь не попросил иначе. Сохраняй смысл, тон и форматирование. Выводи только перевод, без пояснений.",
  },
  {
    id: "editor",
    title: "Редактор текста",
    prompt: "Ты — внимательный редактор. Исправляй орфографию, пунктуацию и стиль, делай текст понятнее, не меняя смысла. Сначала дай исправленный текст, потом коротко перечисли главные правки.",
  },
  {
    id: "coder",
    title: "Помощник программиста",
    prompt: `Ты — опытный программист. Объясняй код, находи ошибки и предлагай исправления. Код давай в блоках с указанием языка. ${BASE}`,
  },
];

export function skillById(id: string | null | undefined): Skill {
  return SKILLS.find((s) => s.id === id) ?? SKILLS[0]!;
}

/** The text of a user message with text attachments appended (pictures go separately). */
export function composeUserText(text: string, attachments: Attachment[]): string {
  const blocks = attachments
    .filter((a) => a.kind === "text" && a.content)
    .map((a) => `Вложение «${a.name}»${a.truncated ? " (показана часть)" : ""}:\n\`\`\`\n${a.content}\n\`\`\``);
  return [text.trim(), ...blocks].filter(Boolean).join("\n\n");
}

/** The conversation for the model: the skill's instructions first, then the stored messages. */
export function buildApiMessages(skill: Skill, history: StoredMsg[]): ApiMsg[] {
  return [
    { role: "system", content: skill.prompt, images: [] },
    ...history.filter((m) => m.content.trim() || m.images.length).map((m) => ({ role: m.role, content: m.content, images: m.images })),
  ];
}

/** A chat title from its first message: one line, no longer than ~48 characters. */
export function titleFrom(text: string): string {
  const line = text.replace(/\s+/g, " ").trim();
  if (!line) return "Новый чат";
  return line.length > 48 ? `${line.slice(0, 47).trimEnd()}…` : line;
}

/** Bytes for people: "850 МБ", "2,6 ГБ". */
export function formatBytes(bytes: number): string {
  const gb = bytes / 2 ** 30;
  if (gb >= 1) return `${gb.toFixed(1).replace(".", ",")} ГБ`;
  return `${Math.max(1, Math.round(bytes / 2 ** 20))} МБ`;
}

/** The latest progress of each file of one installation, keyed by stage. */
export type InstallProgress = Partial<Record<ModelProgress["stage"], ModelProgress>>;

/** Adds an event to an installation's progress: the files download at the same time. */
export function withProgress(all: InstallProgress | undefined, p: ModelProgress): InstallProgress {
  return { ...all, [p.stage]: p };
}

/** One line under a model while it installs: all its files together. */
export function installProgressText(all: InstallProgress): string {
  const parts = Object.values(all);
  const total = parts.reduce((s, p) => s + p.total, 0);
  if (parts.length && parts.every((p) => p.check)) {
    const done = parts.reduce((s, p) => s + p.done, 0);
    return `Проверка подлинности ${total ? Math.min(100, Math.floor((done / total) * 100)) : 0}%`;
  }
  // a file being checked has finished downloading
  const done = parts.reduce((s, p) => s + (p.check ? p.total : p.done), 0);
  if (!total) return done ? `Загрузка: ${formatBytes(done)}` : "Готовлю загрузку…";
  return `Загрузка: ${formatBytes(done)} из ${formatBytes(total)} (${Math.min(100, Math.floor((done / total) * 100))}%)`;
}

/** Overall share of an installation, 0..1: downloading is the bulk, the sha256 pass the last tenth. */
export function installFraction(all: InstallProgress): number {
  const parts = Object.values(all);
  const total = parts.reduce((s, p) => s + p.total, 0);
  if (!total) return 0;
  const got = parts.reduce((s, p) => s + (p.check ? p.total * 0.9 + Math.min(p.done, p.total) * 0.1 : Math.min(p.done, p.total) * 0.9), 0);
  return Math.min(1, got / total);
}

/** A short label for how a model fits this computer. */
export function fitLabel(level: FitLevel): string {
  return { good: "Подходит", slow: "Будет медленно", tight: "Впритык по памяти", no: "Не потянет" }[level];
}

/** Hardware summary line for the models window. */
export function hardwareLine(hw: { ramTotal: number; diskFree: number | null; gpu: boolean; gpuName: string }): string {
  const parts = [`память ${Math.round(hw.ramTotal / 2 ** 30)} ГБ`];
  if (hw.diskFree !== null) parts.push(`свободно на диске ${formatBytes(hw.diskFree)}`);
  parts.push(hw.gpu ? `ускорение: ${hw.gpuName}` : "без видеокарты — модели работают на процессоре");
  return parts.join(", ");
}

/** Whether a message can be sent: some text or an attachment, and no answer running. */
export function canSend(text: string, attachments: Attachment[], busy: boolean, hasModel: boolean): boolean {
  return hasModel && !busy && (text.trim().length > 0 || attachments.some((a) => a.content));
}
