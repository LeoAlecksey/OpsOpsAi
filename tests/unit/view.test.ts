import { describe, expect, it } from "vitest";
import { aboutLine, escapeHtml, modelStatus, promoCard, PROMO_PLACEHOLDER } from "../../src/view";

describe("promo slot", () => {
  it("shows the placeholder until cards are loaded", () => {
    expect(promoCard([])).toEqual(PROMO_PLACEHOLDER);
  });

  it("shows the first loaded card", () => {
    const card = { title: "Курс", text: "Описание" };
    expect(promoCard([card, { title: "x", text: "y" }])).toEqual(card);
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

describe("about line", () => {
  it("names the version and platform", () => {
    expect(aboutLine({ name: "OpsOpsAi", version: "0.1.0", os: "linux", arch: "x86_64" })).toBe("OpsOpsAi 0.1.0 · linux x86_64");
  });

  it("says when there is no backend", () => {
    expect(aboutLine(null)).toBe("Запущено в браузере, без бэкенда");
  });
});
