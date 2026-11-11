// Pure helpers for the UI: decisions live here and are covered by tests/unit, the DOM code in
// shell.ts only applies them.

import type { Entitlements } from "./ipc";

/** Text for HTML markup built from strings. */
export function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export type ProFeature = "skills" | "folders";

/** A Pro feature shown with a lock on the free plan. */
export function locked(e: Entitlements, feature: ProFeature): boolean {
  return !e[feature];
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

/** What the promo slot shows: nothing on Pro, otherwise the first card or the placeholder. */
export function promoCard(e: Entitlements, cards: readonly PromoCard[]): PromoCard | null {
  if (!e.showPromo) return null;
  return cards[0] ?? PROMO_PLACEHOLDER;
}

/** Status line under the model picker. */
export function modelStatus(installed: boolean): string {
  return installed ? "Модель готова" : "Модель не установлена";
}
