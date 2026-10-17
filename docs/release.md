# Сборка и выпуск

## CI (`.github/workflows/ci.yml`)

На каждый push и PR, матрица macOS / Windows / Ubuntu 22.04:

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Тест на железе (`#[ignore]`) в CI не запускается.
2. `npm ci`, `npm run build`, `npm test`.
3. `npm run tauri build`: на macOS universal (`--target universal-apple-darwin`), на Linux — AppImage и deb.
4. Установщики — в артефактах прогона (Actions → прогон → Artifacts): `stlink-rust-ui-macOS` (dmg, app), `-Windows` (msi, nsis `-setup.exe`), `-Linux` (AppImage, deb).

Права `GITHUB_TOKEN` — `contents: read`.

## Выпуск релиза (`.github/workflows/release.yml`)

1. Поднять версию в трёх местах (должны совпадать):
   - `Cargo.toml` → `[workspace.package] version`
   - `src-tauri/tauri.conf.json` → `version`
   - `package.json` → `version` (`npm version X.Y.Z --no-git-tag-version`)
2. Проверить локально: `node scripts/check-version.mjs vX.Y.Z`.
3. Закоммитить, поставить тег и запушить его:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

4. Workflow сначала сверяет версию с тегом (при расхождении падает и пишет, какой файл не совпал), затем `tauri-apps/tauri-action` собирает приложение на трёх ОС и создаёт **черновик** GitHub Release «stlink-rust-ui vX.Y.Z» с установщиками.
5. Проверить черновик (Releases), дописать заметки и нажать «Publish release».

Права `GITHUB_TOKEN`: `contents: write` только у job сборки (ему нужно создать релиз).

## Подпись

Без секретов:
- **macOS** — ad-hoc подпись (`bundle.macOS.signingIdentity: "-"` в `src-tauri/tauri.macos.conf.json`). Скачанное приложение не считается «повреждённым», но Gatekeeper предупредит о неизвестном разработчике: первый запуск через правый клик → «Открыть» (или Системные настройки → Конфиденциальность и безопасность → «Всё равно открыть»).
- **Windows** — установщики без подписи, SmartScreen предупреждает при запуске (см. [windows.md](windows.md)).

Секреты добавляются в Settings → Secrets and variables → Actions → New repository secret. Шаги подписи в `release.yml` включаются, только если секреты заданы.

### Apple: подпись Developer ID и нотаризация

Нужен Apple Developer Program и сертификат «Developer ID Application».

| Секрет | Что |
|---|---|
| `APPLE_CERTIFICATE` | Сертификат с ключом, `.p12` в base64: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | Пароль от `.p12` |
| `APPLE_SIGNING_IDENTITY` | Имя сертификата, например `Developer ID Application: Name (TEAMID)` (`security find-identity -v -p codesigning`) |
| `APPLE_ID` | Apple ID для нотаризации |
| `APPLE_PASSWORD` | App-specific password (appleid.apple.com → Sign-In and Security → App-Specific Passwords) |
| `APPLE_TEAM_ID` | Team ID (developer.apple.com → Membership) |

С первыми тремя приложение подписывается Developer ID. Со всеми шестью Tauri ещё и нотаризует его.

### Windows: подпись кода

Нужен сертификат подписи кода (OV/EV) в виде `.pfx`.

| Секрет | Что |
|---|---|
| `WINDOWS_CERTIFICATE` | `.pfx` в base64: `[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))` |
| `WINDOWS_CERTIFICATE_PASSWORD` | Пароль от `.pfx` |

Workflow импортирует сертификат в хранилище раннера и передаёт его отпечаток в `tauri build` (`bundle.windows.certificateThumbprint`, SHA-256, метка времени DigiCert). EV-сертификаты на токене так не работают: для них нужен `signCommand` с облачным сервисом подписи. Это отдельная задача.

## Локально

- macOS universal: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`, затем `npm run tauri build -- --target universal-apple-darwin`; результат — в `target/universal-apple-darwin/release/bundle/{macos,dmg}`.
- Проверка workflow: `actionlint .github/workflows/*.yml` (`brew install actionlint`).
