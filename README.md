# OpsOpsAi

Бесплатное настольное приложение для локального ИИ: установил, выбрал модель под свой компьютер,
общаешься без облака. Дальше — скиллы, работа с папками и расширения (агенты).

**Статус: 0.1.0, ранний доступ.** Есть окно приложения с тёмным интерфейсом чата, автообновление,
диагностика и хранение настроек. Локальной модели и чата ещё нет — они в следующих версиях.
Сайт: `site/` (GitHub Pages).

## Стек

Tauri 2 · Rust (бэкенд) · TypeScript + Vite без фреймворка (интерфейс).
Платформы MVP: macOS arm64, Windows x64, Linux x64 и arm64.

## Запуск для разработки

Нужны Node.js 22+, Rust (версия закреплена в `rust-toolchain.toml`, rustup поставит сам) и
системные зависимости Tauri: <https://tauri.app/start/prerequisites/>.
На Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev build-essential`.

```bash
npm ci
npm run tauri dev        # приложение с горячей перезагрузкой интерфейса
npm run dev              # только интерфейс в браузере (без бэкенда, план Free)
```

## Проверки

```bash
npx tsc --noEmit                     # типы
npm test                             # vitest: чистая логика интерфейса
npm audit --audit-level=high
cd src-tauri && cargo test --lib && cargo clippy --lib --tests -- -D warnings
```

CI (`.github/workflows/ci.yml`) прогоняет то же самое и собирает неподписанные пакеты для
четырёх платформ.

## Релизы

Выпуск автоматический (`.github/workflows/release.yml`): если в `main` пришла версия, для которой
ещё нет релиза, CI собирает все платформы, подписывает файлы обновлений, собирает `latest.json`
и публикует GitHub Release. Установленные копии видят обновление сами.

```bash
npm run release -- 0.2.0   # первый запуск создаст release-notes/0.2.0.md — заполните и повторите
git push                   # дальше всё делает CI
```

Нужно один раз: ключ подписи обновлений —
`npx tauri signer generate -w ~/.tauri/opsopsai.key`; публичный ключ — в
`src-tauri/tauri.conf.json` → `plugins.updater.pubkey`, приватный ключ и пароль — в secrets
репозитория `TAURI_SIGNING_PRIVATE_KEY` и `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## Устройство

```
src/                интерфейс
  main.ts           запуск: сведения о приложении → каркас
  shell.ts          разметка окна: боковая панель, чат, слот рекомендаций
  view.ts           чистые функции интерфейса (покрыты tests/unit)
  ipc.ts            типизированные вызовы команд бэкенда
  icons.ts          собственные иконки
  styles.css        тёмная тема
src-tauri/src/      бэкенд
  lib.rs            сборка приложения и список команд
  store.rs          JSON-настройки с атомарной записью и резервной копией
  diag.rs           журнал, паники, сторож зависаний UI
  updater.rs        проверка и установка обновлений
site/               сайт продукта
scripts/            выпуск версии, сборка latest.json
release-notes/      описания релизов (текст релиза и окна обновления)
tests/unit/         vitest
```

## Лицензия

Исходный код открыт **только для чтения**: смотреть можно, копировать, изменять,
переиспользовать и распространять — нельзя. Подробно — [`LICENSE`](LICENSE).
Готовые сборки OpsOpsAi бесплатны. Сторонние компоненты — [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
