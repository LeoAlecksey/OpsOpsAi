// A fake Tauri backend for UI tests: answers the app's commands from window.__MOCK_STATE__ and lets
// a test emit backend events with window.__mockEmit(event, payload). Loaded before the app.
(() => {
  const callbacks = new Map();
  const listeners = new Map(); // event → Map(eventId → callbackId)
  let next = 1;
  const S = (window.__MOCK_STATE__ ??= {});
  S.calls = [];

  const emit = (event, payload) => {
    for (const cb of (listeners.get(event) ?? new Map()).values()) callbacks.get(cb)?.({ event, id: 0, payload });
  };
  window.__mockEmit = emit;

  const models = () => {
    const items = S.models ?? [];
    const sel = S.selected ?? null;
    return {
      hardware: { os: "linux", arch: "x86_64", ramTotal: 16 * 2 ** 30, diskFree: 200 * 2 ** 30, cpuThreads: 8, gpu: false, gpuName: "" },
      selected: sel,
      running: null,
      recommended: items.filter((m) => m.fit.level === "good"),
      possible: items.filter((m) => m.fit.level === "slow" || m.fit.level === "tight"),
      hidden: S.hidden ?? 0,
    };
  };

  const handlers = {
    app_info: () => ({ name: "OpsOpsAi", version: "0.2.0", os: "linux", arch: "x86_64" }),
    log_ui: () => null,
    update_check: () => ({ current: "0.2.0", available: false, version: null, notes: null }),
    models_list: () => models(),
    model_install: ({ id }) => {
      // installation runs in the background: progress, then "installed"
      setTimeout(() => emit("model-progress", { id, stage: "model", done: 2 ** 30, total: 2 * 2 ** 30, check: false }), 30);
      setTimeout(() => {
        const m = S.models.find((x) => x.id === id);
        m.installed = true;
        S.selected ??= id;
        emit("model-installed", { id });
      }, 120);
      return null;
    },
    model_select: ({ id }) => {
      S.selected = id;
      return null;
    },
    chats_list: () => (S.chats ?? []).map((c) => ({ id: c.id, title: c.title, updated: c.updated })),
    chat_save: ({ chat }) => {
      S.chats = [chat, ...(S.chats ?? []).filter((c) => c.id !== chat.id)];
      return null;
    },
    chat_load: ({ id }) => S.chats.find((c) => c.id === id),
    chat_delete: ({ id }) => {
      S.chats = S.chats.filter((c) => c.id !== id);
      return null;
    },
    chat_send: async ({ id, messages }) => {
      S.lastMessages = messages;
      const ev = `chat-${id}`;
      emit(ev, { status: "starting" });
      for (const piece of S.answer ?? ["Готово."]) {
        await new Promise((r) => setTimeout(r, 15));
        emit(ev, { delta: piece });
      }
      emit(ev, { done: true });
      return null;
    },
    chat_stop: () => null,
    attach_pick: () => S.attachments ?? [],
    "plugin:event|listen": ({ event, handler }) => {
      const id = next++;
      if (!listeners.has(event)) listeners.set(event, new Map());
      listeners.get(event).set(id, handler);
      return id;
    },
    "plugin:event|unlisten": ({ event, eventId }) => {
      listeners.get(event)?.delete(eventId);
      return null;
    },
  };

  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => {
      const id = next++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    invoke: async (cmd, args = {}) => {
      S.calls.push(cmd);
      const h = handlers[cmd];
      if (!h) throw new Error(`mock: no handler for ${cmd}`);
      return h(args);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event, id) => listeners.get(event)?.delete(id),
  };
})();
