// The models window: what this computer runs well (recommended), what it can run slower or tight on
// memory, installation with progress, choosing and removing. Models it cannot run are not offered.

import { icons } from "./icons";
import {
  inTauri,
  logUi,
  modelCancel,
  modelInstall,
  modelRemove,
  modelSelect,
  modelsList,
  onModelEvents,
  type ModelItem,
  type ModelList,
} from "./ipc";
import { fitLabel, formatBytes, hardwareLine, installFraction, installProgressText, withProgress, type InstallProgress } from "./chatlogic";
import { escapeHtml } from "./view";

export interface ModelsApi {
  /** Opens the window and refreshes the list. */
  open(): void;
  /** The current list (null in a browser without the backend). */
  current(): ModelList | null;
  /** Called with the fresh list whenever it changes (installed, removed, selected). */
  onChange(fn: (list: ModelList | null) => void): void;
}

function card(m: ModelItem, selected: string | null, progress: InstallProgress | undefined, error: string | undefined): string {
  const badges = [
    `<span class="badge">${formatBytes(m.size)}</span>`,
    `<span class="badge">от ${m.ramGb} ГБ памяти</span>`,
    m.vision ? `<span class="badge badge-vision">${icons.image}картинки</span>` : "",
    m.activeB < m.paramsB ? `<span class="badge">MoE</span>` : "",
    m.fit.level !== "good" ? `<span class="badge badge-warn">${fitLabel(m.fit.level)}</span>` : "",
  ].join("");
  let actions: string;
  if (m.installing || progress) {
    const pct = progress ? Math.round(installFraction(progress) * 100) : 0;
    actions = `
      <div class="install">
        <div class="bar"><span style="width:${pct}%"></span></div>
        <div class="install-row"><span class="muted">${progress ? escapeHtml(installProgressText(progress)) : "Готовлю загрузку…"}</span>
        <button class="btn btn-small" data-act="cancel" data-id="${m.id}" type="button">Отмена</button></div>
      </div>`;
  } else if (m.installed) {
    actions = `
      ${m.id === selected ? `<span class="in-use">Используется</span>` : `<button class="btn btn-accent btn-small" data-act="select" data-id="${m.id}" type="button">Выбрать</button>`}
      <button class="icon-btn" data-act="remove" data-id="${m.id}" type="button" title="Удалить модель с диска">${icons.trash}</button>`;
  } else {
    actions = `<button class="btn btn-small" data-act="install" data-id="${m.id}" type="button">Установить</button>`;
  }
  return `
    <article class="model-card${m.id === selected ? " selected" : ""}">
      <div class="model-head"><h4>${escapeHtml(m.title)}</h4><div class="model-actions">${actions}</div></div>
      <p>${escapeHtml(m.description)}</p>
      <div class="badges">${badges}</div>
      ${m.fit.why ? `<p class="why">${escapeHtml(m.fit.why)}</p>` : ""}
      ${error ? `<p class="error">${escapeHtml(error)}</p>` : ""}
      <p class="license">Лицензия: ${escapeHtml(m.license)}</p>
    </article>`;
}

export function mountModels(root: HTMLElement): ModelsApi {
  const dialog = root.querySelector<HTMLDialogElement>("#models")!;
  const body = root.querySelector<HTMLElement>("#models-body")!;
  const hwLine = root.querySelector<HTMLElement>("#models-hw")!;
  let list: ModelList | null = null;
  const progress = new Map<string, InstallProgress>();
  const errors = new Map<string, string>();
  const listeners: ((l: ModelList | null) => void)[] = [];
  let confirmRemove: string | null = null;

  const render = () => {
    if (!list) {
      body.innerHTML = `<p class="muted">${inTauri() ? "Не удалось получить список моделей." : "Модели доступны только в установленном приложении."}</p>`;
      return;
    }
    hwLine.textContent = `Этот компьютер: ${hardwareLine(list.hardware)}.`;
    const sel = list.selected;
    const section = (title: string, note: string, items: ModelItem[]) =>
      items.length ? `<h3>${title}</h3><p class="muted section-note">${note}</p>${items.map((m) => card(m, sel, progress.get(m.id), errors.get(m.id))).join("")}` : "";
    body.innerHTML =
      section("Рекомендуем для вашего компьютера", "Пойдут быстро и не потеснят другие программы.", list.recommended) +
      section("Тоже можно поставить", "Запустятся, но будут медленнее или займут почти всю память.", list.possible) +
      (list.hidden ? `<p class="muted hidden-note">Ещё ${list.hidden} ${list.hidden === 1 ? "модель не показана" : "модели не показаны"}: этот компьютер их не потянет.</p>` : "") +
      (!list.recommended.length && !list.possible.length ? `<p class="muted">Подходящих моделей нет: не хватает памяти или места на диске.</p>` : "");
    if (confirmRemove) {
      const b = body.querySelector<HTMLButtonElement>(`[data-act="remove"][data-id="${confirmRemove}"]`);
      if (b) {
        b.outerHTML = `<button class="btn btn-small btn-danger" data-act="remove-yes" data-id="${confirmRemove}" type="button">Удалить?</button>`;
      }
    }
  };

  const refresh = async () => {
    try {
      list = await modelsList();
    } catch (e) {
      logUi("error", `models_list: ${String(e)}`);
      list = null;
    }
    render();
    listeners.forEach((fn) => fn(list));
  };

  body.addEventListener("click", (ev) => {
    const b = (ev.target as HTMLElement).closest<HTMLButtonElement>("button[data-act]");
    if (!b) return;
    const id = b.dataset.id!;
    const act = b.dataset.act;
    const run = (p: Promise<void>, what: string) =>
      p.catch((e) => {
        errors.set(id, String(e));
        logUi("warn", `${what} ${id}: ${String(e)}`);
      }).finally(() => void refresh());
    if (act === "install") {
      errors.delete(id);
      progress.set(id, {});
      render();
      void run(modelInstall(id), "install");
    } else if (act === "cancel") {
      void modelCancel(id);
    } else if (act === "select") {
      void run(modelSelect(id), "select");
    } else if (act === "remove") {
      confirmRemove = id;
      render();
    } else if (act === "remove-yes") {
      confirmRemove = null;
      void run(modelRemove(id), "remove");
    }
  });

  void onModelEvents({
    progress: (p) => {
      progress.set(p.id, withProgress(progress.get(p.id), p));
      // a light update while bytes arrive: no full re-render needed when the window is closed
      if (dialog.open) render();
    },
    installed: (id) => {
      progress.delete(id);
      void refresh();
    },
    error: (id, error) => {
      progress.delete(id);
      // a cancel by the user is not an error worth showing
      if (!error.includes("отменена")) errors.set(id, error);
      void refresh();
    },
  });

  root.querySelector("#close-models")!.addEventListener("click", () => dialog.close());
  dialog.addEventListener("click", (ev) => {
    if (ev.target === dialog) dialog.close();
  });
  dialog.addEventListener("close", () => (confirmRemove = null));

  void refresh();
  return {
    open: () => {
      dialog.showModal();
      void refresh();
    },
    current: () => list,
    onChange: (fn) => listeners.push(fn),
  };
}
