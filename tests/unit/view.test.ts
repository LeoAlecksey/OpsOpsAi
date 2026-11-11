import { describe, expect, it } from "vitest";
import type { Entitlements } from "../../src/ipc";
import { escapeHtml, locked, modelStatus, promoCard, PROMO_PLACEHOLDER } from "../../src/view";

const free: Entitlements = { plan: "free", showPromo: true, skills: false, folders: false };
const pro: Entitlements = { plan: "pro", showPromo: false, skills: true, folders: true };

describe("promo slot", () => {
  it("shows the placeholder on Free until cards are loaded", () => {
    expect(promoCard(free, [])).toEqual(PROMO_PLACEHOLDER);
  });

  it("shows the first loaded card", () => {
    const card = { title: "Курс", text: "Описание" };
    expect(promoCard(free, [card, { title: "x", text: "y" }])).toEqual(card);
  });

  it("is empty on Pro even with cards", () => {
    expect(promoCard(pro, [{ title: "Курс", text: "Описание" }])).toBeNull();
  });
});

describe("pro features", () => {
  it("are locked on Free and open on Pro", () => {
    expect(locked(free, "skills")).toBe(true);
    expect(locked(free, "folders")).toBe(true);
    expect(locked(pro, "skills")).toBe(false);
    expect(locked(pro, "folders")).toBe(false);
  });
});

describe("escapeHtml", () => {
  it("neutralises markup in card texts", () => {
    expect(escapeHtml(`<img src=x onerror="a()">&'`)).toBe("&lt;img src=x onerror=&quot;a()&quot;&gt;&amp;&#39;");
  });
});

describe("model status", () => {
  it("says whether a model is installed", () => {
    expect(modelStatus(false)).toBe("Модель не установлена");
    expect(modelStatus(true)).toBe("Модель готова");
  });
});
