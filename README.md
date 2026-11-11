# OpsOpsAi

Бесплатное настольное приложение для локального ИИ: установил, выбрал модель под свой компьютер,
общаешься без облака. Дальше — скиллы, работа с папками и расширения (агенты).

**Статус: Phase 1, каркас.** Есть окно приложения с тёмным интерфейсом чата, диагностика
и хранение настроек. Локальной модели и чата ещё нет — это Phase 2.
Решения и план: [`docs/`](docs/), правила работы: [`CLAUDE.md`](CLAUDE.md).

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
четырёх платформ. Публикации релизов пока нет.

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
tests/unit/         vitest
docs/               продукт, архитектура, безопасность, план, решения
```

## Лицензия

Проприетарная, см. [`LICENSE`](LICENSE). Сторонние компоненты — [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
