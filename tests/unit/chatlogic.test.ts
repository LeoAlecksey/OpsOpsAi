import { describe, expect, it } from "vitest";
import type { Attachment, ModelProgress, StoredMsg } from "../../src/ipc";
import {
  SKILLS,
  buildApiMessages,
  canSend,
  composeUserText,
  fitLabel,
  formatBytes,
  hardwareLine,
  installFraction,
  installProgressText,
  skillById,
  titleFrom,
} from "../../src/chatlogic";

const att = (over: Partial<Attachment>): Attachment => ({ name: "a.txt", kind: "text", content: "x", files: 1, truncated: false, problem: null, ...over });
const msg = (role: "user" | "assistant", content: string, images: string[] = []): StoredMsg => ({ role, content, images, attachments: [] });

describe("skills", () => {
  it("have unique ids and an unknown id falls back to the plain chat", () => {
    expect(new Set(SKILLS.map((s) => s.id)).size).toBe(SKILLS.length);
    expect(skillById("nope").id).toBe("chat");
    expect(skillById(null).id).toBe("chat");
    expect(skillById("translator").title).toBe("Переводчик");
  });
});

describe("what goes to the model", () => {
  it("text attachments are appended as fenced blocks, pictures are not", () => {
    const text = composeUserText("Сделай выжимку", [att({ name: "отчёт.md", content: "# Итоги" }), att({ name: "p.png", kind: "image", content: "data:image/png;base64,AA" })]);
    expect(text).toContain("Сделай выжимку");
    expect(text).toContain("Вложение «отчёт.md»:\n```\n# Итоги\n```");
    expect(text).not.toContain("data:image");
  });

  it("a cut attachment says so", () => {
    expect(composeUserText("", [att({ truncated: true })])).toContain("(показана часть)");
  });

  it("the skill's instructions go first, empty messages are dropped, pictures are kept", () => {
    const api = buildApiMessages(skillById("editor"), [msg("user", "Привет", ["data:image/png;base64,AA"]), msg("assistant", ""), msg("assistant", "Здравствуйте")]);
    expect(api[0]).toEqual({ role: "system", content: skillById("editor").prompt, images: [] });
    expect(api.map((m) => m.role)).toEqual(["system", "user", "assistant"]);
    expect(api[1]!.images).toHaveLength(1);
  });
});

describe("titles", () => {
  it("come from the first message in one line", () => {
    expect(titleFrom("  Как   выбрать\nмодель?  ")).toBe("Как выбрать модель?");
    expect(titleFrom("")).toBe("Новый чат");
    const long = titleFrom("а".repeat(100));
    expect(long.length).toBe(48);
    expect(long.endsWith("…")).toBe(true);
  });
});

describe("sizes and progress", () => {
  it("bytes read like people write them", () => {
    expect(formatBytes(2.6 * 2 ** 30)).toBe("2,6 ГБ");
    expect(formatBytes(850 * 2 ** 20)).toBe("850 МБ");
    expect(formatBytes(10)).toBe("1 МБ");
  });

  it("each stage says what is happening", () => {
    const p = (over: Partial<ModelProgress>): ModelProgress => ({ id: "m", stage: "model", done: 2 ** 30, total: 4 * 2 ** 30, check: false, ...over });
    expect(installProgressText(p({}))).toBe("Модель: 1,0 ГБ из 4,0 ГБ (25%)");
    expect(installProgressText(p({ check: true, done: 2 * 2 ** 30 }))).toBe("Модель: проверка подлинности 50%");
    expect(installProgressText(p({ stage: "engine", total: 0, done: 5 * 2 ** 20 }))).toBe("Движок: 5 МБ");
  });

  it("the overall bar only moves forward across stages", () => {
    const at = (stage: ModelProgress["stage"], done: number, check = false) => installFraction({ id: "m", stage, done, total: 100, check });
    const seq = [at("engine", 0), at("engine", 100), at("engine", 100, true), at("model", 0), at("model", 50), at("model", 100, true), at("vision", 0), at("vision", 100, true)];
    for (let i = 1; i < seq.length; i++) expect(seq[i]!).toBeGreaterThanOrEqual(seq[i - 1]!);
    expect(seq.at(-1)).toBeCloseTo(1);
  });
});

describe("labels", () => {
  it("fit levels are named for people", () => {
    expect(fitLabel("good")).toBe("Подходит");
    expect(fitLabel("tight")).toBe("Впритык по памяти");
  });

  it("the hardware line mentions memory, disk and acceleration", () => {
    expect(hardwareLine({ ramTotal: 16 * 2 ** 30, diskFree: 120 * 2 ** 30, gpu: true, gpuName: "Apple Silicon (Metal)" })).toBe(
      "память 16 ГБ, свободно на диске 120,0 ГБ, ускорение: Apple Silicon (Metal)",
    );
    expect(hardwareLine({ ramTotal: 8 * 2 ** 30, diskFree: null, gpu: false, gpuName: "" })).toContain("на процессоре");
  });
});

describe("sending", () => {
  it("needs a model, something to send and no running answer", () => {
    expect(canSend("привет", [], false, true)).toBe(true);
    expect(canSend("   ", [], false, true)).toBe(false);
    expect(canSend("", [att({})], false, true)).toBe(true);
    expect(canSend("", [att({ content: "" })], false, true)).toBe(false);
    expect(canSend("привет", [], true, true)).toBe(false);
    expect(canSend("привет", [], false, false)).toBe(false);
  });
});
