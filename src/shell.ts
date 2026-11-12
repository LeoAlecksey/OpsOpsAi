// The app frame: sidebar (history, skill, promo slot) and the chat area. The chat itself lives in
// chat-ui.ts, the models window in models-ui.ts; this file draws the frame and wires settings.

import { icons } from "./icons";
import type { AppInfo } from "./ipc";
import { donateOpen, logsOpen, logUi, updateCheck, updateInstall, type UpdateInfo } from "./ipc";
import { aboutLine, escapeHtml, promoCard, updateProgressText, updateStatusText, type PromoCard } from "./view";
import { SKILLS } from "./chatlogic";
import { mountChat } from "./chat-ui";
import { mountModels } from "./models-ui";

function skillBlock(): string {
  const options = SKILLS.map((k) => `<option value="${k.id}">${escapeHtml(k.title)}</option>`).join("");
  return `
    <section class="side-block">
      <label class="side-label" for="skill">${icons.spark}<span>Скилл</span></label>
      <select class="picker" id="skill" title="Инструкции для модели в этом чате">${options}</select>
    </section>`;
}

function promoBlock(card: PromoCard): string {
  return `
    <aside class="promo" id="promo" aria-label="Рекомендации">
      <div class="promo-title">${escapeHtml(card.title)}</div>
      <div class="promo-text">${escapeHtml(card.text)}</div>
    </aside>`;
}

function sidebar(info: AppInfo | null): string {
  const version = info ? `v${escapeHtml(info.version)}` : "браузер";
  return `
    <nav class="sidebar" id="sidebar" aria-label="Чаты">
      <div class="brand">
        ${icons.logo}<span class="brand-name">OpsOpsAi</span>
        <button class="icon-btn" id="collapse" type="button" title="Скрыть панель">${icons.panel}</button>
      </div>
      <button class="new-chat" id="new-chat" type="button">${icons.plus}<span>Новый чат</span></button>
      <label class="search">${icons.search}<input type="search" id="search" placeholder="Поиск по чатам" /></label>
      <div class="history" id="history">
        <div class="history-empty">Чатов пока нет</div>
      </div>
      ${skillBlock()}
      ${promoBlock(promoCard([]))}
      <button class="update-pill" id="update-pill" type="button" hidden>${icons.update}<span></span></button>
      <div class="side-footer">
        <button class="icon-btn" id="open-settings" type="button" title="Настройки">${icons.settings}</button>
        <button class="donate-link" id="donate-side" type="button" title="Поддержать разработку через ЮMoney">${icons.heart}<span>Поддержать</span></button>
        <span class="version">${version}</span>
      </div>
    </nav>`;
}

function chat(): string {
  return `
    <main class="chat">
      <header class="topbar">
        <button class="icon-btn" id="expand" type="button" title="Показать панель" hidden>${icons.panel}</button>
        <button class="model-picker" id="model-picker" type="button" title="Выбрать или установить модель">
          ${icons.chip}<span class="model-name" id="model-name">Модель не выбрана</span>${icons.chevron}
        </button>
        <span class="status-dot" id="status-dot" aria-hidden="true"></span>
        <span class="status" id="status">Проверяю модели…</span>
      </header>
      <section class="messages" id="messages" aria-live="polite"></section>
      <section class="empty" id="empty">
        <h1>Чем помочь?</h1>
        <p id="empty-text">Локальный ИИ работает на этом компьютере. Ничего не уходит в интернет.</p>
        <button class="btn btn-accent" id="empty-models" type="button" hidden>${icons.chip}<span>Выбрать и установить модель</span></button>
      </section>
      <form class="composer" id="composer">
        <div class="attachments" id="attachments" hidden></div>
        <div class="composer-box">
          <textarea id="prompt" rows="1" placeholder="Спросите что-нибудь…"></textarea>
          <div class="composer-row">
            <button class="icon-btn" id="attach-file" type="button" title="Прикрепить файл или картинку">${icons.paperclip}</button>
            <button class="icon-btn" id="attach-folder" type="button" title="Прикрепить папку">${icons.folder}</button>
            <span class="spacer"></span>
            <button class="stop" id="stop" type="button" title="Остановить ответ" hidden>${icons.stop}</button>
            <button class="send" id="send" type="submit" title="Отправить (Enter)" disabled>${icons.send}</button>
          </div>
        </div>
        <div class="hint">Ответы генерируются локально и не покидают компьютер. Enter — отправить, Shift+Enter — новая строка.</div>
      </form>
    </main>`;
}

function modelsDialog(): string {
  return `
    <dialog class="settings models" id="models" aria-labelledby="models-title">
      <header class="settings-head">
        <h2 id="models-title">Модели</h2>
        <button class="icon-btn" id="close-models" type="button" title="Закрыть">${icons.close}</button>
      </header>
      <p class="models-hw muted" id="models-hw"></p>
      <div class="models-body" id="models-body"><p class="muted">Загружаю список…</p></div>
    </dialog>`;
}

function settingsDialog(info: AppInfo | null): string {
  return `
    <dialog class="settings" id="settings" aria-labelledby="settings-title">
      <header class="settings-head">
        <h2 id="settings-title">Настройки</h2>
        <button class="icon-btn" id="close-settings" type="button" title="Закрыть">${icons.close}</button>
      </header>
      <section class="settings-section">
        <h3>О приложении</h3>
        <p class="muted">${escapeHtml(aboutLine(info))}</p>
        <button class="btn" id="open-logs" type="button" ${info ? "" : "disabled"}>${icons.logs}<span>Открыть папку журналов</span></button>
      </section>
      <section class="settings-section" id="updates">
        <h3>Обновления</h3>
        <p class="muted" id="update-status">${info ? "Проверка ещё не выполнялась" : escapeHtml(updateStatusText(null))}</p>
        <p class="update-notes" id="update-notes" hidden></p>
        <div class="row">
          <button class="btn" id="update-check" type="button" ${info ? "" : "disabled"}>${icons.update}<span>Проверить обновления</span></button>
          <button class="btn btn-accent" id="update-install" type="button" hidden><span>Установить и перезапустить</span></button>
        </div>
      </section>
      <section class="settings-section donate-card">
        <h3>Поддержать проект</h3>
        <p class="muted">OpsOpsAi бесплатный. Если приложение вам полезно, можно поддержать разработку.</p>
        <button class="btn btn-accent" id="donate-settings" type="button">${icons.heart}<span>Поддержать через ЮMoney</span></button>
      </section>
    </dialog>`;
}

/** Runs a button action and reports a failure in the log instead of failing silently. */
function action(run: () => Promise<void>, what: string): () => void {
  return () => void run().catch((e) => logUi("error", `${what}: ${String(e)}`));
}

export function mountShell(root: HTMLElement, info: AppInfo | null): void {
  root.innerHTML = `<div class="app" id="layout">${sidebar(info)}${chat()}</div>${settingsDialog(info)}${modelsDialog()}`;
  const layout = root.querySelector<HTMLElement>("#layout")!;
  const expand = root.querySelector<HTMLButtonElement>("#expand")!;
  const setCollapsed = (on: boolean) => {
    layout.classList.toggle("collapsed", on);
    expand.hidden = !on;
  };
  root.querySelector("#collapse")!.addEventListener("click", () => setCollapsed(true));
  expand.addEventListener("click", () => setCollapsed(false));

  const settings = root.querySelector<HTMLDialogElement>("#settings")!;
  root.querySelector("#open-settings")!.addEventListener("click", () => settings.showModal());
  root.querySelector("#close-settings")!.addEventListener("click", () => settings.close());
  // a click on the backdrop (outside the panel) closes the dialog
  settings.addEventListener("click", (ev) => {
    if (ev.target === settings) settings.close();
  });
  const donate = action(donateOpen, "donate");
  root.querySelector("#donate-side")!.addEventListener("click", donate);
  root.querySelector("#donate-settings")!.addEventListener("click", donate);
  root.querySelector("#open-logs")!.addEventListener("click", action(logsOpen, "logs"));

  mountUpdates(root, settings, info !== null);
  const models = mountModels(root);
  mountChat(root, models);
}

/** Update check and install: the settings section plus a small pill in the sidebar when one is available. */
function mountUpdates(root: HTMLElement, settings: HTMLDialogElement, inApp: boolean): void {
  const status = root.querySelector<HTMLElement>("#update-status")!;
  const notes = root.querySelector<HTMLElement>("#update-notes")!;
  const checkBtn = root.querySelector<HTMLButtonElement>("#update-check")!;
  const installBtn = root.querySelector<HTMLButtonElement>("#update-install")!;
  const pill = root.querySelector<HTMLButtonElement>("#update-pill")!;

  const show = (u: UpdateInfo | null) => {
    status.textContent = updateStatusText(u);
    const available = !!u?.available && !!u.version;
    installBtn.hidden = !available;
    notes.hidden = !(available && u?.notes);
    notes.textContent = u?.notes ?? "";
    pill.hidden = !available;
    pill.querySelector("span")!.textContent = available ? `Обновление ${u!.version}` : "";
  };

  const check = async (quiet: boolean) => {
    checkBtn.disabled = true;
    if (!quiet) status.textContent = "Проверяю…";
    try {
      show(await updateCheck());
    } catch (e) {
      // a background check stays silent: no network is normal for a local-first app
      if (!quiet) status.textContent = String(e);
      else logUi("info", `update check: ${String(e)}`);
    } finally {
      checkBtn.disabled = false;
    }
  };

  checkBtn.addEventListener("click", () => void check(false));
  pill.addEventListener("click", () => settings.showModal());
  installBtn.addEventListener("click", () => {
    installBtn.disabled = checkBtn.disabled = true;
    status.textContent = "Скачиваю обновление…";
    updateInstall((p) => (status.textContent = updateProgressText(p))).catch((e) => {
      status.textContent = String(e);
      installBtn.disabled = checkBtn.disabled = false;
    });
  });

  // one quiet check shortly after start, so launching the app is not slowed down
  if (inApp) setTimeout(() => void check(true), 5000);
}
