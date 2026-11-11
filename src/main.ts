import { appInfo, logUi } from "./ipc";
import { mountShell } from "./shell";

window.addEventListener("error", (e) => logUi("error", `${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => logUi("error", `unhandled: ${String(e.reason)}`));

async function boot(): Promise<void> {
  const root = document.getElementById("app")!;
  const info = await appInfo().catch(() => null);
  mountShell(root, info);
}

void boot();
