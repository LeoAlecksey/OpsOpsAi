import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const MOCK = readFileSync(fileURLToPath(new URL("./mock.js", import.meta.url)), "utf8");
const GB = 2 ** 30;

const model = (id: string, title: string, level: string, extra: Record<string, unknown> = {}) => ({
  id,
  title,
  family: "Qwen3.5",
  paramsB: 4,
  activeB: 4,
  ramGb: 8,
  license: "Apache-2.0",
  description: `Описание ${title}`,
  vision: true,
  size: 3 * GB,
  installed: false,
  installing: false,
  fit: { level, why: level === "good" ? "" : "памяти впритык" },
  ...extra,
});

async function open(page: Page, state: Record<string, unknown>) {
  await page.addInitScript(`window.__MOCK_STATE__ = ${JSON.stringify(state)};`);
  await page.addInitScript(MOCK);
  await page.goto("/");
}

test("first run without a model invites to install one", async ({ page }) => {
  await open(page, { models: [model("qwen3.5-4b", "Qwen3.5 4B", "good")] });
  await expect(page.locator("#model-name")).toHaveText("Модель не выбрана");
  await expect(page.locator("#empty-models")).toBeVisible();
  await expect(page.locator("#send")).toBeDisabled();
});

test("the models window puts recommended first and leaves out what will not run", async ({ page }) => {
  await open(page, {
    models: [model("qwen3.5-4b", "Qwen3.5 4B", "good"), model("qwen3.5-9b", "Qwen3.5 9B", "tight", { paramsB: 9 })],
    hidden: 3,
  });
  await page.locator("#model-picker").click();
  const body = page.locator("#models-body");
  await expect(body.locator("h3").first()).toHaveText("Рекомендуем для вашего компьютера");
  await expect(body.locator("h3").nth(1)).toHaveText("Тоже можно поставить");
  await expect(body.locator(".model-card").first()).toContainText("Qwen3.5 4B");
  await expect(body).toContainText("памяти впритык");
  await expect(body).toContainText("Ещё 3 модели не показаны");
});

test("installing a model shows progress and makes it the chat model", async ({ page }) => {
  await open(page, { models: [model("qwen3.5-4b", "Qwen3.5 4B", "good")] });
  await page.locator("#empty-models").click();
  await page.locator('[data-act="install"]').click();
  await expect(page.locator(".install")).toBeVisible();
  await expect(page.locator(".in-use")).toHaveText("Используется");
  await page.locator("#close-models").click();
  await expect(page.locator("#model-name")).toHaveText("Qwen3.5 4B");
});

test("a message gets a streamed Markdown answer and lands in the history", async ({ page }) => {
  await open(page, {
    models: [model("qwen3.5-4b", "Qwen3.5 4B", "good", { installed: true })],
    selected: "qwen3.5-4b",
    answer: ["**Привет!**", " Чем", " помочь?\n\n", "```js\nconsole.log(1)\n```"],
  });
  await page.locator("#skill").selectOption("translator");
  await page.locator("#prompt").fill("Переведи: доброе утро");
  await page.locator("#prompt").press("Enter");
  const answer = page.locator(".msg.assistant .md");
  await expect(answer.locator("strong")).toHaveText("Привет!");
  await expect(answer.locator("pre code")).toContainText("console.log(1)");
  await expect(page.locator(".history-item")).toHaveCount(1);
  await expect(page.locator(".history-open")).toHaveText("Переведи: доброе утро");
  // the skill's instructions went first
  const sent = await page.evaluate(() => (window as unknown as { __MOCK_STATE__: { lastMessages: { role: string; content: string }[] } }).__MOCK_STATE__.lastMessages);
  expect(sent[0]!.role).toBe("system");
  expect(sent[0]!.content).toContain("переводчик");
});

test("model output is never trusted as HTML", async ({ page }) => {
  await open(page, {
    models: [model("qwen3.5-4b", "Qwen3.5 4B", "good", { installed: true })],
    selected: "qwen3.5-4b",
    answer: ['<img src=x onerror="window.__pwned=1"><script>window.__pwned=2</script>текст'],
  });
  await page.locator("#prompt").fill("тест");
  await page.locator("#send").click();
  await expect(page.locator(".msg.assistant .md")).toContainText("текст");
  expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned)).toBeUndefined();
  expect(await page.locator(".msg.assistant .md script, .msg.assistant .md img[onerror]").count()).toBe(0);
});

test("attachments show as chips and their text goes to the model", async ({ page }) => {
  await open(page, {
    models: [model("qwen3.5-4b", "Qwen3.5 4B", "good", { installed: true })],
    selected: "qwen3.5-4b",
    attachments: [{ name: "заметки.md", kind: "text", content: "# План", files: 1, truncated: false, problem: null }],
  });
  await page.locator("#attach-file").click();
  await expect(page.locator("#attachments .chip")).toContainText("заметки.md");
  await page.locator("#prompt").fill("Сделай выжимку");
  await page.locator("#send").click();
  await expect(page.locator(".msg.user .msg-files")).toContainText("заметки.md");
  const sent = await page.evaluate(() => (window as unknown as { __MOCK_STATE__: { lastMessages: { content: string }[] } }).__MOCK_STATE__.lastMessages);
  expect(sent.at(-1)!.content).toContain("Вложение «заметки.md»");
});
