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
  withProgress,
  type InstallProgress,
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

  const ev = (stage: ModelProgress["stage"], done: number, total: number, check = false): ModelProgress => ({ id: "m", stage, done, total, check });
  const G = 2 ** 30;

  it("files downloading at once read as one installation", () => {
    let all = withProgress(undefined, ev("model", G, 3 * G));
    all = withProgress(all, ev("vision", 0, G));
    expect(installProgressText(all)).toBe("Загрузка: 1,0 ГБ из 4,0 ГБ (25%)");
    // the next event of a file replaces its previous one, not adds to it
    all = withProgress(all, ev("vision", G / 2, G));
    expect(installProgressText(all)).toBe("Загрузка: 1,5 ГБ из 4,0 ГБ (37%)");
    // a file in its check counts as downloaded
    all = withProgress(all, ev("vision", 0, G, true));
    expect(installProgressText(all)).toBe("Загрузка: 2,0 ГБ из 4,0 ГБ (50%)");
    all = withProgress(all, ev("model", 3 * G, 3 * G, true));
    expect(installProgressText(all)).toBe("Проверка подлинности 75%");
    expect(installProgressText({})).toBe("Готовлю загрузку…");
    expect(installProgressText(withProgress(undefined, ev("engine", 5 * 2 ** 20, 0)))).toBe("Загрузка: 5 МБ");
  });

  it("the overall bar only moves forward and ends full", () => {
    const steps: ModelProgress[] = [
      ev("model", 0, 100), ev("engine", 0, 10), ev("vision", 0, 30), ev("engine", 10, 10),
      ev("engine", 10, 10, true), ev("model", 50, 100), ev("vision", 30, 30, true), ev("model", 100, 100, true),
    ];
    let all: InstallProgress = {};
    const seq = steps.map((p) => installFraction((all = withProgress(all, p))));
    // a newly seen file adds to the total, so only compare once all three are known
    for (let i = 3; i < seq.length; i++) expect(seq[i]!).toBeGreaterThanOrEqual(seq[i - 1]!);
    expect(seq.at(-1)).toBeCloseTo(1);
    expect(installFraction({})).toBe(0);
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
