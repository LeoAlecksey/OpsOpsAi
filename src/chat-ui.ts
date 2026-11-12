// The chat: history in the sidebar, the conversation, sending with a streamed answer, stopping,
// attachments (files, folders, pictures) and skills.

import { icons } from "./icons";
import {
  attachPick,
  chatDelete,
  chatLoad,
  chatSave,
  chatSend,
  chatsList,
  chatStop,
  inTauri,
  logUi,
  type Attachment,
  type Chat,
  type ChatMeta,
  type ModelList,
  type StoredMsg,
} from "./ipc";
import { buildApiMessages, canSend, composeUserText, skillById, titleFrom } from "./chatlogic";
import { renderMarkdown } from "./markdown";
import type { ModelsApi } from "./models-ui";
import { escapeHtml } from "./view";

function newChat(skill: string): Chat {
  const now = Date.now();
  return { id: crypto.randomUUID(), title: "Новый чат", created: now, updated: now, model: null, skill, messages: [] };
}

/** The selected model, if it is installed. */
function selectedModel(list: ModelList | null) {
  if (!list?.selected) return null;
  return [...list.recommended, ...list.possible].find((m) => m.id === list.selected && m.installed) ?? null;
}

export function mountChat(root: HTMLElement, models: ModelsApi): void {
  const $ = <T extends HTMLElement>(sel: string) => root.querySelector<T>(sel)!;
  const historyEl = $("#history");
  const searchEl = $<HTMLInputElement>("#search");
  const skillEl = $<HTMLSelectElement>("#skill");
  const messagesEl = $("#messages");
  const emptyEl = $("#empty");
  const emptyText = $("#empty-text");
  const emptyModels = $<HTMLButtonElement>("#empty-models");
  const promptEl = $<HTMLTextAreaElement>("#prompt");
  const sendEl = $<HTMLButtonElement>("#send");
  const stopEl = $<HTMLButtonElement>("#stop");
  const attachEl = $("#attachments");
  const modelName = $("#model-name");
  const statusEl = $("#status");
  const statusDot = $("#status-dot");

  let chats: ChatMeta[] = [];
  let chat: Chat = newChat(skillEl.value);
  let attachments: Attachment[] = [];
  let busy = false;
  let list: ModelList | null = models.current();
  let confirmDelete: string | null = null;

  // ---------- top bar and empty state ----------

  const setStatus = (text: string, state: "ok" | "warn" | "busy") => {
    statusEl.textContent = text;
    statusDot.dataset.state = state;
  };

  const renderModel = () => {
    const m = selectedModel(list);
    modelName.textContent = m ? m.title : "Модель не выбрана";
    if (!inTauri()) setStatus("Запущено в браузере, без бэкенда", "warn");
    else if (busy) return;
    else if (m) setStatus(m.vision ? "Готова, понимает картинки" : "Готова", "ok");
    else setStatus("Установите модель, чтобы начать", "warn");
    emptyModels.hidden = !!m || !inTauri();
    emptyText.textContent = m
      ? "Локальный ИИ работает на этом компьютере. Ничего не уходит в интернет."
      : "Чтобы начать, установите модель. Приложение покажет, какие подойдут вашему компьютеру.";
    updateSend();
  };

  const updateSend = () => {
    sendEl.disabled = !canSend(promptEl.value, attachments, busy, !!selectedModel(list));
    sendEl.hidden = busy;
    stopEl.hidden = !busy;
  };

  // ---------- history ----------

  const renderHistory = () => {
    const q = searchEl.value.trim().toLowerCase();
    const shown = chats.filter((c) => !q || c.title.toLowerCase().includes(q));
    if (!shown.length) {
      historyEl.innerHTML = `<div class="history-empty">${q ? "Ничего не найдено" : "Чатов пока нет"}</div>`;
      return;
    }
    historyEl.innerHTML = shown
      .map(
        (c) => `
        <div class="history-item${c.id === chat.id ? " active" : ""}" data-id="${c.id}">
          <button class="history-open" type="button" data-act="open" data-id="${c.id}" title="${escapeHtml(c.title)}">${escapeHtml(c.title)}</button>
          ${
            confirmDelete === c.id
              ? `<button class="history-del confirm" type="button" data-act="delete-yes" data-id="${c.id}">Удалить?</button>`
              : `<button class="history-del" type="button" data-act="delete" data-id="${c.id}" title="Удалить чат">${icons.trash}</button>`
          }
        </div>`,
      )
      .join("");
  };

  const reloadHistory = async () => {
    try {
      chats = await chatsList();
    } catch (e) {
      logUi("error", `chats_list: ${String(e)}`);
    }
    renderHistory();
  };

  historyEl.addEventListener("click", (ev) => {
    const b = (ev.target as HTMLElement).closest<HTMLButtonElement>("button[data-act]");
    if (!b || busy) return;
    const id = b.dataset.id!;
    if (b.dataset.act === "open") {
      confirmDelete = null;
      chatLoad(id)
        .then((c) => {
          chat = c;
          skillEl.value = skillById(c.skill).id;
          attachments = [];
          renderAttachments();
          renderMessages();
          renderHistory();
          renderModel();
        })
        .catch((e) => logUi("error", `chat_load: ${String(e)}`));
    } else if (b.dataset.act === "delete") {
      confirmDelete = id;
      renderHistory();
    } else if (b.dataset.act === "delete-yes") {
      confirmDelete = null;
      chatDelete(id)
        .then(() => {
          if (chat.id === id) startNew();
          return reloadHistory();
        })
        .catch((e) => logUi("error", `chat_delete: ${String(e)}`));
    }
  });
  searchEl.addEventListener("input", renderHistory);

  const startNew = () => {
    if (busy) return;
    chat = newChat(skillEl.value);
    attachments = [];
    renderAttachments();
    renderMessages();
    renderHistory();
    renderModel();
    promptEl.focus();
  };
  $("#new-chat").addEventListener("click", startNew);

  skillEl.addEventListener("change", () => {
    chat.skill = skillEl.value;
  });

  // ---------- messages ----------

  const messageHtml = (m: StoredMsg, i: number) => {
    if (m.role === "user") {
      const files = m.attachments.length ? `<div class="msg-files">${m.attachments.map((n) => `<span class="chip">${icons.file}${escapeHtml(n)}</span>`).join("")}</div>` : "";
      const imgs = m.images.length ? `<div class="msg-images">${m.images.map((src) => `<img src="${src}" alt="Прикреплённая картинка" />`).join("")}</div>` : "";
      // the attachment text is part of the content for the model; the bubble shows only what was typed
      const typed = m.content.split("\n\nВложение «")[0] ?? m.content;
      return `<div class="msg user">${imgs}${files}${typed ? `<div class="bubble">${escapeHtml(typed)}</div>` : ""}</div>`;
    }
    return `
      <div class="msg assistant" data-i="${i}">
        <div class="md">${m.content ? renderMarkdown(m.content) : `<span class="typing" aria-label="Модель думает"><i></i><i></i><i></i></span>`}</div>
        ${m.content ? `<button class="msg-copy" type="button" data-copy="${i}" title="Скопировать ответ">${icons.copy}</button>` : ""}
      </div>`;
  };

  const renderMessages = () => {
    const hasMessages = chat.messages.length > 0;
    emptyEl.hidden = hasMessages;
    messagesEl.hidden = !hasMessages;
    messagesEl.innerHTML = `<div class="messages-inner">${chat.messages.map(messageHtml).join("")}</div>`;
    messagesEl.scrollTop = messagesEl.scrollHeight;
  };

  messagesEl.addEventListener("click", (ev) => {
    const b = (ev.target as HTMLElement).closest<HTMLButtonElement>("button[data-copy]");
    if (!b) return;
    const m = chat.messages[Number(b.dataset.copy)];
    if (m) void navigator.clipboard.writeText(m.content).then(() => (b.dataset.done = "1"));
  });

  /** Redraws only the last answer while it streams (cheap enough for every few pieces). */
  let pending = false;
  const updateLast = (error?: string) => {
    if (pending && !error) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      const i = chat.messages.length - 1;
      const el = messagesEl.querySelector<HTMLElement>(`.msg.assistant[data-i="${i}"]`);
      const m = chat.messages[i];
      if (!el || !m) return;
      const nearBottom = messagesEl.scrollHeight - messagesEl.scrollTop - messagesEl.clientHeight < 120;
      el.outerHTML = messageHtml(m, i);
      if (error) {
        messagesEl.querySelector(`.msg.assistant[data-i="${i}"]`)?.insertAdjacentHTML("beforeend", `<p class="error">${escapeHtml(error)}</p>`);
      }
      if (nearBottom) messagesEl.scrollTop = messagesEl.scrollHeight;
    });
  };

  // ---------- attachments ----------

  const renderAttachments = () => {
    attachEl.hidden = attachments.length === 0;
    const vision = !!selectedModel(list)?.vision;
    attachEl.innerHTML = attachments
      .map((a, i) => {
        const warn = a.problem ?? (a.kind === "image" && !vision ? "эта модель не понимает картинки" : a.truncated ? "взята часть" : "");
        const icon = a.kind === "image" ? icons.image : a.files > 1 ? icons.folder : icons.file;
        const label = a.files > 1 ? `${a.name} (${a.files} файлов)` : a.name;
        return `<span class="chip${a.problem ? " chip-bad" : ""}" title="${escapeHtml(warn)}">${icon}${escapeHtml(label)}${warn ? `<em>${escapeHtml(warn)}</em>` : ""}<button type="button" data-rm="${i}" title="Убрать">${icons.close}</button></span>`;
      })
      .join("");
    updateSend();
  };
  attachEl.addEventListener("click", (ev) => {
    const b = (ev.target as HTMLElement).closest<HTMLButtonElement>("button[data-rm]");
    if (!b) return;
    attachments.splice(Number(b.dataset.rm), 1);
    renderAttachments();
  });
  const pick = (folder: boolean) =>
    attachPick(folder)
      .then((got) => {
        attachments.push(...got);
        renderAttachments();
      })
      .catch((e) => logUi("error", `attach: ${String(e)}`));
  $("#attach-file").addEventListener("click", () => void pick(false));
  $("#attach-folder").addEventListener("click", () => void pick(true));

  // ---------- composer and sending ----------

  const autosize = () => {
    promptEl.style.height = "auto";
    promptEl.style.height = `${Math.min(promptEl.scrollHeight, 220)}px`;
  };
  promptEl.addEventListener("input", () => {
    autosize();
    updateSend();
  });
  promptEl.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter" && !ev.shiftKey && !ev.isComposing) {
      ev.preventDefault();
      if (!sendEl.disabled) void send();
    }
  });
  $("#composer").addEventListener("submit", (ev) => {
    ev.preventDefault();
    if (!sendEl.disabled) void send();
  });
  stopEl.addEventListener("click", () => void chatStop(chat.id));

  const send = async () => {
    const model = selectedModel(list);
    if (!model || busy) return;
    const usable = attachments.filter((a) => a.content && (a.kind === "text" || model.vision));
    const text = promptEl.value;
    const user: StoredMsg = {
      role: "user",
      content: composeUserText(text, usable),
      images: usable.filter((a) => a.kind === "image").map((a) => a.content),
      attachments: usable.filter((a) => a.kind === "text").map((a) => (a.files > 1 ? `${a.name} (${a.files} файлов)` : a.name)),
    };
    if (chat.messages.length === 0) chat.title = titleFrom(text || usable[0]?.name || "");
    chat.model = model.id;
    chat.skill = skillEl.value;
    chat.messages.push(user);
    const answer: StoredMsg = { role: "assistant", content: "", images: [], attachments: [] };
    chat.messages.push(answer);
    promptEl.value = "";
    autosize();
    attachments = [];
    renderAttachments();
    busy = true;
    renderModel();
    renderMessages();
    setStatus("Запускаю модель…", "busy");

    let failed: string | undefined;
    try {
      await chatSend(chat.id, buildApiMessages(skillById(chat.skill), chat.messages.slice(0, -1)), (e) => {
        if (e.status === "starting") setStatus("Загружаю модель в память…", "busy");
        if (e.status === "answering") setStatus("Отвечает…", "busy");
        if (e.delta) {
          answer.content += e.delta;
          updateLast();
        }
        if (e.error) failed = e.error;
      });
    } catch (e) {
      failed = failed ?? String(e);
    }
    busy = false;
    if (failed) logUi("warn", `chat: ${failed}`);
    updateLast(failed);
    chat.updated = Date.now();
    renderModel();
    try {
      await chatSave(chat);
      await reloadHistory();
    } catch (e) {
      logUi("error", `chat_save: ${String(e)}`);
    }
  };

  // ---------- wiring ----------

  $("#model-picker").addEventListener("click", () => models.open());
  emptyModels.addEventListener("click", () => models.open());
  models.onChange((l) => {
    list = l;
    renderModel();
    renderAttachments();
  });

  renderMessages();
  renderModel();
  void reloadHistory();
}
