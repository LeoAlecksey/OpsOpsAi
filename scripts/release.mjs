#!/usr/bin/env node
// Raises the version for the next release: `npm run release -- 0.2.0`.
// Needs release-notes/<version>.md (created from a template on the first run), then updates
// package.json, tauri.conf.json, Cargo.toml and Cargo.lock and commits. After `git push` to main the release workflow builds
// and publishes v<version> by itself (a version that already has a release is not built again).
// Origin: adapted from OpsDeck scripts/release.mjs (same author).
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Использование: npm run release -- 0.2.0");
  process.exit(1);
}
const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
if (git("status", "--porcelain")) {
  console.error("Есть незакоммиченные изменения — закоммитьте или уберите их перед релизом.");
  process.exit(1);
}
if (git("tag", "-l", `v${version}`)) {
  console.error(`Тег v${version} уже существует.`);
  process.exit(1);
}

// release notes first: they become the text of the release and of the in-app update dialog
const notes = `release-notes/${version}.md`;
if (!existsSync(notes)) {
  mkdirSync("release-notes", { recursive: true });
  writeFileSync(notes, "## Что нового\n\n- \n");
  console.error(`Создан ${notes}: опишите изменения и запустите команду ещё раз.`);
  process.exit(1);
}

const edit = (file, fn) => writeFileSync(file, fn(readFileSync(file, "utf8")));
edit("package.json", (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`));
edit("package-lock.json", (s) => s.replace(/("name": "opsopsai",\s*"version": )"[^"]+"/g, `$1"${version}"`));
edit("src-tauri/tauri.conf.json", (s) => s.replace(/"version": "[^"]+"/, `"version": "${version}"`));
edit("src-tauri/Cargo.toml", (s) => s.replace(/^version = "[^"]+"/m, `version = "${version}"`));
edit("src-tauri/Cargo.lock", (s) => s.replace(/(name = "opsopsai"\nversion = )"[^"]+"/, `$1"${version}"`));

git("add", "package.json", "package-lock.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", notes);
if (git("status", "--porcelain")) git("commit", "-m", `Версия ${version}`);
console.log(`✓ версия ${version}. Проверьте ${notes}, затем git push — релиз соберётся сам.`);
