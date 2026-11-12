// Pure helpers for the UI: decisions live here and are covered by tests/unit, the DOM code in
// shell.ts only applies them.

/** Text for HTML markup built from strings. */
export function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** One card of the own promo slot (courses, channels). Loaded from a signed feed later. */
export interface PromoCard {
  title: string;
  text: string;
}

/** Placeholder until the feed with real cards is connected. */
export const PROMO_PLACEHOLDER: PromoCard = {
  title: "Рекомендации",
  text: "Здесь появятся курсы и материалы по работе с локальным ИИ.",
};

/** What the promo slot shows: the first loaded card or the placeholder. */
export function promoCard(cards: readonly PromoCard[]): PromoCard {
  return cards[0] ?? PROMO_PLACEHOLDER;
}

/** "OpsOpsAi 0.1.0 · linux x86_64", or a note when running without the backend. */
export function aboutLine(info: { name: string; version: string; os: string; arch: string } | null): string {
  return info ? `${info.name} ${info.version} · ${info.os} ${info.arch}` : "Запущено в браузере, без бэкенда";
}

/** Bytes as "12.3 МБ". */
export function formatMb(bytes: number): string {
  return `${(bytes / 1048576).toFixed(1)} МБ`;
}

/** Progress line while an update downloads and installs. */
export function updateProgressText(p: { downloaded?: number; total?: number | null; installing?: boolean }): string {
  if (p.installing) return "Устанавливаю, приложение перезапустится…";
  const done = p.downloaded ?? 0;
  if (p.total) return `Скачано ${formatMb(done)} из ${formatMb(p.total)} (${Math.min(100, Math.round((done / p.total) * 100))}%)`;
  return `Скачано ${formatMb(done)}`;
}

/** The result of an update check in words. */
export function updateStatusText(info: { current: string; available: boolean; version: string | null } | null): string {
  if (!info) return "Обновления проверяются только в установленном приложении";
  return info.available && info.version ? `Доступна версия ${info.version}` : `Установлена последняя версия (${info.current})`;
}
