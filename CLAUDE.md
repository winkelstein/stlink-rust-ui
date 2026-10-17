# stlink-rust-ui

Desktop-приложение для STM32: чтение/запись прошивки через ST-Link (клоны V2, оригинальные V3) и через USB DFU (DfuSe, system bootloader, без ST-Link).

Полные требования: @docs/PRD-stlink-rust-ui-2026-10-17.md

## Текущий этап
- Этап 1 — библиотека `stlink/` + консольный стенд `stlink-cli/` (Cargo workspace). GUI (Tauri 2 + React 19 + TS в `src-tauri/`, `src/`) — Этап 2, пока не трогать.
- M0–M2 готовы; M3 для H7 готов (прямая запись регистров flash по SWD пакетами по 1 КБ, `write <file.bin>`; программирование ~87 КБ/с @1.8 МГц, ~137 КБ/с @4 МГц; пакеты из 0xFF пропускаются). Дальше в M3: F4 (F427), затем F1, L4, G4, G0, F0, F7.
- Стенд H743: прошивка уходит в low-power с выключенными debug-тактами → AP0 отдаёт устаревшие данные со статусом OK. Нужен `--keep-debug-in-low-power` (один раз после включения питания) или `--connect-under-reset`.
- На стенде NRST от ST-Link таргет НЕ сбрасывает (st-flash: «NRST is not connected»): `--connect-under-reset` сам откатывается на AIRCR.SYSRESETREQ. Под сбросом AP0 на H7 мёртв — никаких read-modify-write там (DEMCR портился → прошивка висела в DebugMon).

## Стенд
- macOS (Apple Silicon); Windows и Linux — НЕ в v1.
- Программатор: клон ST-Link V2 `0483:3748` (работает с `st-info` и `openocd` — использовать их как эталон).
- Таргет: STM32H743VIT6 (Cortex-M7, chip id `0x450`, DBGMCU_IDCODE по `0x5C001000`, 2 МБ flash, 2 банка, сектора 128 КБ, запись по 32 байта).
- DFU-таргет позже: STM32F427 (`0483:df11`).

## Команды
- Сборка/проверка: `cargo check --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- Стенд: `cargo run -p stlink-cli -- list | probe | regs | mem <addr> [count] [--ap N] | read <file> [--addr] [--size] | write <file.bin> [--addr] [--no-verify] [--no-reset]` (`--connect-under-reset` — сброс таргета при подключении; `--keep-debug-in-low-power` — DBGMCU_CR, на H7 через AP2, держится до выключения питания; `--freq <kHz>` (по умолчанию 1800, всегда выставляется явно — ST-Link помнит частоту прошлой сессии); флаги `-v/-vv/-vvv` — уровень логов, `-vvv` — hex-трейс каждого USB-пакета)
- Эталон: `st-info --probe`

## Правила
- USB только через `nusb` (не rusb/libusb). Логи — `tracing`. Ошибки — `thiserror` в библиотеке, `anyhow` только в CLI.
- Никаких `panic!`/`unwrap()` в библиотеке на путях, зависящих от устройства: в release `panic = "abort"`. Все USB-операции — с таймаутом; отключение кабеля → `Error::Disconnected`, не зависание.
- Feature gating по версии прошивки ST-Link: не слать команды, которые прошивка не поддерживает (клоны часто со старыми версиями).
- Данные о чипах — таблицей (по мотивам `config/chips/*.chip` из stlink, BSD-3), а не хардкодом в логике.
- Справочники: stlink (BSD-3 — заимствовать можно, с указанием в `NOTICE`), probe-rs (MIT/Apache). **OpenOCD — GPL-2.0: код не копировать**, только сверяться с протоколом.
- Форматы прошивки: только `.bin` (позже `.hex`; `.elf` не поддерживаем).
- Запрещено трогать Option Bytes/RDP. Mass erase — только по явному запросу.
- Логику без железа (разбор ответов, декодирование версий/регистров, DfuSe-дескрипторы) покрывать unit-тестами.
