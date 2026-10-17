# stlink-rust-ui

Desktop-приложение для STM32: чтение/запись прошивки через ST-Link (клоны V2, оригинальные V3) и через USB DFU (DfuSe, system bootloader, без ST-Link).

Полные требования: @docs/PRD-stlink-rust-ui-2026-10-17.md

## Текущий этап
- Этап 1 (библиотека `stlink/` + стенд `stlink-cli/`) — M0–M3 для H7 готовы и проверены на стенде. Следующий шаг — **Этап 2: GUI** (Tauri 2 + React 19 + TS в `src-tauri/`, `src/`), см. «Заметки к Этапу 2».
- Этап 2: фронтенд и бэкенд Tauri подключены к библиотеке (hot-plug, identify/чтение/запись, быстрая запись).
- Отложено: M3 для других семейств (F4/F427, затем F1, L4, G4, G0, F0, F7), M4 DFU (нет железа).
- Режим записи flash решён (открытый вопрос 1 PRD): прямая запись регистров по SWD пакетами по 1 КБ, flash loader не нужен. H743: программирование ~87 КБ/с @1.8 МГц, ~137 КБ/с @4 МГц; стирание ~0.95 с на сектор 128 КБ.

## Стенд
- macOS (Apple Silicon); Windows и Linux — НЕ в v1.
- Программатор: клон ST-Link V2 `0483:3748`, прошивка V2J46S7 (эталоны: `st-info`, `st-flash`, `openocd`).
- Таргет: STM32H743VIT6 (Cortex-M7, chip id `0x450`, DBGMCU_IDCODE по `0x5C001000` / через AP2 по `0xE00E1000`, 2 МБ flash, 2 банка, сектора 128 КБ, запись по 32 байта).
- Особенности стенда (важно для отладки):
  - Прошивка уходит в low-power с выключенными debug-тактами (`DBGMCU_CR = 0`) → AP0 отдаёт устаревшие данные **со статусом OK**. Нужен `keep_debug_in_low_power` (один раз после включения питания) или `under_reset`.
  - NRST от ST-Link таргет НЕ сбрасывает → connect under reset откатывается на AIRCR.SYSRESETREQ (как st-flash).
  - `st-info --connect-under-reset` оставляет ядро остановленным на векторе сброса.
  - ST-Link помнит частоту SWD прошлой сессии → всегда выставлять явно.
- DFU-таргет позже: STM32F427 (`0483:df11`).

## Команды
- Сборка/проверка: `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p stlink`
- Регрессия на железе: `scripts/hw-test.sh [firmware.bin]` (без аргумента — не пишет flash; с файлом — пишет и сверяет). Прогонять после изменений в `stlink/`.
- Стенд: `cargo run -p stlink-cli -- list | probe | regs | mem <addr> [count] [--ap N] | read <file> [--addr] [--size] | write <file.bin> [--addr] [--no-verify] [--no-reset]`
  - `--connect-under-reset`, `--keep-debug-in-low-power`, `--freq <kHz>` (по умолчанию 1800), `-v/-vv/-vvv` (`-vvv` — hex-трейс каждого USB-пакета).
- Коммиты: английский, conventional commits; push — только по просьбе.

## Архитектура библиотеки `stlink/`
| Модуль | Что |
|---|---|
| `usb` | discovery (`list_probes`, нормализация серийников клонов), `transport` (bulk + таймауты + trace, детект отключения по шине) |
| `probe` | `StLink`: протокол ST-Link, версия/`ProbeCapabilities` (feature gating), команды, multi-AP |
| `arm` | память (`read_memory`, `read_u32`/`write_u32` через READMEM/WRITEMEM + статус), halt/run, регистры; `arm/reset.rs` — connect under reset, `reset_and_run` |
| `chips` | таблица чипов из `chips/stlink/*.chip` (stlink, BSD-3) + `chips/extras.txt`; таблицы RDP/DBGMCU по `flash_type` |
| `target` | `identify()` → `TargetInfo`; поиск чипа через debug-AP (H7: AP2); `enable_debug_in_low_power` |
| `session` | `ConnectOptions` → `StLink::connect` → `Connection`; `release` |
| `flash` | `dump`, `read_flash`, `plan_write`, `write_flash` (erase → program → verify), `read_protection`; драйверы через трейт `FlashDriver` (`flash/h7.rs`), геометрия/план — `flash/geometry.rs` |

## Заметки к Этапу 2 (UI)
- **Дизайн:** `docs/design/README.md` (экраны, правила быстрой записи) и `docs/design/Main.dc.html` (вёрстка, CSS-токены светлой/тёмной темы, тексты всех экранов). Переносить в React + TS, сохраняя структуру и токены.
- **Сценарий для GUI** (то же делает CLI, см. `stlink-cli/src/commands.rs`):
  `list_probes()` → `StLink::open(&desc)` → `target_voltage()` → `connect(&ConnectOptions)` → `identify()` → `dump(...)` / `plan_write(...)` + подтверждение (показать `plan.sectors`) → `write_flash(...)` → `reset_and_run()`; для чтения/probe — `release(&connection)`.
- **Потоки:** один рабочий поток (актор) на устройство, команды сериализуются. `StLink`, `Error`, `ProbeDescriptor` — `Send` (проверяется при компиляции в `lib.rs`), не `Sync`. Всё API блокирующее, без async-рантайма. При drop `StLink` выходит из debug-режима.
- **Прогресс и отмена:** колбэк `FnMut(Progress) -> ControlFlow<()>`; в Tauri — слать событие и возвращать `Break`, если взведён флаг отмены (`AtomicBool`). Отмена — между блоками; при `Cancelled`/ошибке в процессе записи flash частично записан, ядро остаётся остановленным.
- **Ошибки:** `stlink::Error` без `Serialize` — в `src-tauri` маппить в `{ kind, message }`. Отдельно обрабатывать: `Disconnected` (переподключить и повторить), `CoreNotAccessible` (предложить «держать отладку в low-power» или «подключение со сбросом»), `FlashProtected`, `Busy` (программатор занят другой программой).
- **Подтверждения:** перед `write_flash` показать, какие сектора будут стёрты (`flash::plan_write`), и что `under_reset`/`keep_debug_in_low_power` меняют состояние таргета (сброс; DBGMCU_CR до выключения питания).
- **Настройки** (PRD §6): частота SWD (по умолчанию `session::DEFAULT_SWD_KHZ`), verify, connect under reset, keep debug in low-power.
- **Логи для багрепортов:** tracing-subscriber в Tauri с уровнем `trace` для `stlink` в файл/буфер; `nusb` при не-trace уровне выключать (дублирует наши ошибки).
- **Сборка:** `src-tauri` — член workspace; его `[profile.release]` живёт в корневом `Cargo.toml` (профили членов игнорируются). Запуск: `npm run tauri dev`; `VITE_BACKEND=mock` — фронтенд на `MockBackend`.
- **Бэкенд GUI (`src-tauri/src`):** `device` (актор на программатор, `Worker`), `hotplug` (`nusb::watch_devices` → пересканирование → событие `device-event`), `dto` (зеркало `src/backend/types.ts`), `error` (`{ kind, message }`), `files` (`.bin`, CRC, notify-наблюдатель за каталогом), `settings` (`quick-write.json` в app data), `logging` (`~/Library/Logs/<id>/stlink-ui.log`, `stlink=trace`). Прогресс — `ipc::Channel`, отмена — `cancel_op(opId)`. Фронтенд: `src/backend/TauriBackend.ts`.
- **Не готово для GUI:** запись для семейств кроме H74x/H75x/H72x/H73x (`Error::Unsupported`), DFU, экраны журнала и настроек.

## Правила
- USB только через `nusb` (не rusb/libusb). Логи — `tracing`. Ошибки — `thiserror` в библиотеке, `anyhow` только в CLI.
- Никаких `panic!`/`unwrap()` в библиотеке на путях, зависящих от устройства: в release `panic = "abort"`. Все USB-операции — с таймаутом; отключение кабеля → `Error::Disconnected`, не зависание.
- Feature gating по версии прошивки ST-Link: не слать команды, которые прошивка не поддерживает (клоны часто со старыми версиями).
- Одиночные слова читать/писать через `read_u32`/`write_u32` (READMEM/WRITEMEM + GETLASTRWSTATUS2), не через `READ_DEBUG_REG`: клоны отдают по нему устаревшие данные со статусом OK.
- Пока таргет в сбросе, AP0 на H7 не отвечает (данные мусорные): там только записи, никаких read-modify-write; значения DHCSR/DEMCR проверять на зарезервированные биты.
- Запись во flash не повторять при WAIT (повторная отправка = повторное программирование слова).
- Данные о чипах — таблицей (по мотивам `config/chips/*.chip` из stlink, BSD-3), а не хардкодом в логике.
- Справочники: stlink (BSD-3 — заимствовать можно, с указанием в `NOTICE`), probe-rs (MIT/Apache). **OpenOCD — GPL-2.0: код не копировать**, только сверяться с протоколом.
- Форматы прошивки: только `.bin` (позже `.hex`; `.elf` не поддерживаем).
- Запрещено трогать Option Bytes/RDP. Mass erase — только по явному запросу.
- Логику без железа (разбор ответов, декодирование версий/регистров, геометрия flash, DfuSe-дескрипторы) покрывать unit-тестами.
