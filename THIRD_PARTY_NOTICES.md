# Third-party notices

OpsOpsAi uses open-source components. Their licenses remain in force for those
components; the OpsOpsAi license (see `LICENSE`) does not apply to them.

Status: **draft for the Phase 1 skeleton.** The full list with license texts will be
generated automatically in CI (cargo-about / npm license report) before the first public
release. Until then this file summarises what was checked on 2026-11-11.

## Code from OpsDeck

Fragments of `src-tauri/src/store.rs` and `src-tauri/src/diag.rs` are adapted from
OpsDeck (https://github.com/LeoAlecksey/opsdeck), written by the same copyright holder.
No code by other OpsDeck contributors is included (checked with `git blame`).

## Rust crates (src-tauri, 427 third-party packages in Cargo.lock)

Checked with `cargo metadata` — every package declares a license:

- Permissive (MIT, Apache-2.0, BSD-3-Clause, ISC, Zlib, Unicode-3.0, Unlicense, 0BSD, CC0)
  — the vast majority, mostly dual "MIT OR Apache-2.0".
- **MPL-2.0** (file-level copyleft): `cssparser`, `cssparser-macros`, `selectors`,
  `dtoa-short`, `option-ext` — pulled in by Tauri/WRY. Used unmodified; MPL-2.0 requires
  keeping their notices and making the source of these files available (they are published
  on crates.io). If any of these files is ever modified, the modified files must be published
  under MPL-2.0.
- Two packages are offered as "MIT OR Apache-2.0 OR LGPL-2.1-or-later"; they are used under
  MIT/Apache-2.0.

Main components: Tauri 2 (MIT OR Apache-2.0), serde (MIT OR Apache-2.0),
tauri-plugin-log (MIT OR Apache-2.0), log (MIT OR Apache-2.0), dirs (MIT OR Apache-2.0).

## npm packages (frontend)

Shipped in the app: `@tauri-apps/api` (Apache-2.0 OR MIT).
Build/test only (not shipped): vite, vitest, typescript, @tauri-apps/cli and their
dependencies — MIT, Apache-2.0, ISC, BSD-3-Clause.

## System libraries (not bundled)

Linux builds link dynamically to the system WebKitGTK and GTK (LGPL-2.1+), declared as
package dependencies (`libwebkit2gtk-4.1-0`, `libgtk-3-0`). Windows uses the system
WebView2 runtime, macOS the system WKWebView.

## Not yet included

The llama.cpp engine and AI models are not part of this skeleton. When added (Phase 2),
their licenses (llama.cpp: MIT; each model: per its model card) must be listed here and
shown to the user before a model is downloaded.
