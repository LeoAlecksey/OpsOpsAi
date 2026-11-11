import { appInfo, entitlements, logUi } from "./ipc";
import { mountShell } from "./shell";

window.addEventListener("error", (e) => logUi("error", `${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => logUi("error", `unhandled: ${String(e.reason)}`));

async function boot(): Promise<void> {
  const root = document.getElementById("app")!;
  const [ent, info] = await Promise.all([entitlements(), appInfo().catch(() => null)]);
  mountShell(root, ent, info);
}

void boot();
