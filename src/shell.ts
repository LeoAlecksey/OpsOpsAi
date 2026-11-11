// The app frame: sidebar (history, context, skill, promo slot) and the chat area.
// Phase 1 is the frame only: actions that need a model are shown disabled and say why.

import { icons } from "./icons";
import type { AppInfo } from "./ipc";
import { donateOpen, logsOpen, logUi } from "./ipc";
import { aboutLine, escapeHtml, modelStatus, promoCard, type PromoCard } from "./view";

function contextBlock(): string {
  return `
    <section class="side-block">
      <div class="side-label">${icons.layers}<span>Контекст</span></div>
      <button class="picker" type="button" disabled title="Появится вместе с локальной моделью">
        <span>Без контекста</span>${icons.chevron}
      </button>
      <button class="picker picker-ghost" type="button" disabled>
        ${icons.folder}<span>Папка или проект</span>
      </button>
    </section>`;
}

function skillBlock(): string {
  return `
    <section class="side-block">
      <div class="side-label">${icons.spark}<span>Скилл</span></div>
      <button class="picker" type="button" disabled title="Появится вместе с локальной моделью">
        <span>Обычный чат</span>${icons.chevron}
      </button>
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
      <button class="new-chat" type="button" disabled title="Появится вместе с локальной моделью">${icons.plus}<span>Новый чат</span></button>
      <label class="search">${icons.search}<input type="search" placeholder="Поиск по чатам" disabled /></label>
      <div class="history">
        <div class="history-empty">Чатов пока нет</div>
      </div>
      ${contextBlock()}
      ${skillBlock()}
      ${promoBlock(promoCard([]))}
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
        <button class="model-picker" type="button" disabled title="Модели появятся в следующей версии">
          ${icons.chip}<span class="model-name">Модель не выбрана</span>${icons.chevron}
        </button>
        <span class="status-dot" aria-hidden="true"></span>
        <span class="status">${modelStatus(false)}</span>
      </header>
      <section class="empty">
        <h1>Чем помочь?</h1>
        <p>Локальный ИИ работает на этом компьютере. Чат станет доступен после установки модели.</p>
      </section>
      <form class="composer" id="composer">
        <div class="composer-box">
          <textarea id="prompt" rows="1" placeholder="Спросите что-нибудь…" disabled></textarea>
          <div class="composer-row">
            <button class="icon-btn" type="button" disabled title="Прикрепить файл">${icons.paperclip}</button>
            <button class="icon-btn" type="button" disabled title="Прикрепить папку">${icons.folder}</button>
            <span class="spacer"></span>
            <button class="send" type="submit" disabled title="Отправить">${icons.send}</button>
          </div>
        </div>
        <div class="hint">Ответы генерируются локально и не покидают компьютер.</div>
      </form>
    </main>`;
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
  root.innerHTML = `<div class="app" id="layout">${sidebar(info)}${chat()}</div>${settingsDialog(info)}`;
  const layout = root.querySelector<HTMLElement>("#layout")!;
  const expand = root.querySelector<HTMLButtonElement>("#expand")!;
  const setCollapsed = (on: boolean) => {
    layout.classList.toggle("collapsed", on);
    expand.hidden = !on;
  };
  root.querySelector("#collapse")!.addEventListener("click", () => setCollapsed(true));
  expand.addEventListener("click", () => setCollapsed(false));
  root.querySelector("#composer")!.addEventListener("submit", (ev) => ev.preventDefault());

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
}
