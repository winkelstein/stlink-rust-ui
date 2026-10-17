# PRD: stlink-rust-ui

> Версия: 0.1 (черновик) · Дата: 2026-10-17 · Автор: Andrew + Claude
> Статус: требования к Этапу 1 (библиотека) согласованы, Этап 2 (GUI) — концептуально

---

## 1. Обзор и цели

**stlink-rust-ui** — удобное, минималистичное и стабильное desktop-приложение для работы с микроконтроллерами STM32:

1. через программатор **ST-Link** (неоригинальные клоны «V2» и оригинальные V3) — идентификация чипа, чтение/запись прошивки;
2. через **USB DFU** встроенного system bootloader STM32 (без ST-Link) — запись/чтение прошивки.

Проект делится на две части:

| Часть | Что | Этап |
|---|---|---|
| `stlink` | Rust-библиотека: USB-транспорт, протокол ST-Link, SWD/ARM debug, flash, DfuSe | 1 |
| `stlink-cli` | Консольная утилита-стенд для проверки библиотеки на железе | 1 |
| `src-tauri` + `src/` | GUI на Tauri 2 + React + TypeScript | 2 |

### Цели
- **G1.** Минимум, с которого всё начинается: вывести как минимум то же, что `st-info --probe`, для большинства STM32.
- **G2.** Надёжно читать и записывать flash через ST-Link (формат `.bin`).
- **G3.** Записывать flash через USB DFU (DfuSe), первый целевой чип — STM32F427.
- **G4.** Современный, минималистичный GUI поверх библиотеки.
- **G5.** Стабильность: никаких зависаний при отключении кабеля, понятные ошибки, корректное восстановление.

### Не-цели (v1)
- Windows и Linux (macOS — единственная платформа v1; см. §11).
- Форматы `.hex` (позже) и `.elf` (не поддерживается вообще).
- GDB-сервер, точки останова, пошаговая отладка, SWO/трассировка.
- JTAG (только SWD), SWIM/STM8.
- Обновление прошивки самого ST-Link.
- Изменение Option Bytes / RDP (только чтение и предупреждения).

---

## 2. Целевая аудитория

- Embedded-разработчики и любители, работающие с STM32 на macOS, которым неудобны STM32CubeProgrammer (тяжёлый, Java) и консольные `st-flash`/`openocd`.
- Владельцы дешёвых клонов ST-Link V2, которым нужна предсказуемая работа без «танцев».
- Пользователи, прошивающие платы через USB DFU (BOOT0=1) без программатора.

---

## 3. Тестовый стенд

| Компонент | Значение | Комментарий |
|---|---|---|
| Хост | macOS (Apple Silicon) | Запуск и проверка на железе — пользователь |
| Программатор | Клон ST-Link «V2» (`0483:3748`) | Корректно работает с `st-info` и `openocd` |
| Таргет №1 | STM32H743VIT6 | Cortex-M7, chip id `0x450`, 2 МБ flash в 2 банках |
| Таргет №2 (DFU) | STM32F427 | Появится позже; DFU через USB OTG FS (PA11/PA12) |
| Эталон | `st-info --probe`, `openocd` | Вывод нашей утилиты сверяется с ними |

**Процесс разработки:** код пишется прямо в папку проекта; сборка проверяется `cargo check --target aarch64-apple-darwin` в Linux-VM (без железа); запуск на железе — пользователь, лог с `-vvv` присылается обратно.

---

## 4. Основные функции

Функции сгруппированы по модулям библиотеки; номера `F*` используются в milestones.

### F1. Обнаружение ST-Link (USB discovery)

- Перечислять USB-устройства через `nusb`, фильтровать по VID `0x0483` и известным PID:

| PID | Модель | EP OUT / IN |
|---|---|---|
| `0x3748` | ST-Link V2 (и большинство клонов) | `0x02` / `0x81` |
| `0x374B`, `0x3752` | ST-Link V2-1 | `0x01` / `0x81` |
| `0x374E`, `0x374F`, `0x3753`, `0x3754` | ST-Link V3 (варианты) | `0x01` / `0x81` |

- Для каждого устройства возвращать `ProbeDescriptor` (см. §6): PID, модель, серийный номер, bus/address.
- **Серийный номер клонов:** у многих клонов строка серийника — сырые байты, а не ASCII. Нормализовать как в `st-info`: если строка непечатаемая — выводить hex-представление байтов.
- Поддержка горячего подключения через `nusb::watch_devices` (события `Connected` / `Disconnected`).
- **Acceptance criteria:**
  - `stlink-cli list` показывает подключённый клон с PID `0x3748` и серийником, совпадающим с `st-info --probe`.
  - Без подключённых устройств — пустой список и код возврата 0, без паники.
  - Отключение во время работы приводит к ошибке `Error::Disconnected`, а не к зависанию.

### F2. Протокол ST-Link (probe layer)

- Команды — 16-байтный пакет в bulk OUT, ответ из bulk IN; все операции с таймаутом (по умолчанию 1000 мс, настраиваемо).
- Ключевые команды (значения сверять с `stlink` и OpenOCD):

| Команда | Код | Назначение |
|---|---|---|
| `GET_VERSION` | `0xF1` | Версия: ST-Link / JTAG / SWIM + VID/PID |
| `GET_VERSION_APIV3` | `0xFB` | Расширенная версия для V3 |
| `GET_CURRENT_MODE` | `0xF5` | Режим: DFU / Mass / Debug / SWIM / Bootloader |
| `DFU` / `DFU_EXIT` | `0xF3` / `0x07` | Выход из DFU-режима самого ST-Link |
| `GET_TARGET_VOLTAGE` | `0xF7` | Напряжение таргета: `2 × adc1 × 1.2 / adc0` |
| `DEBUG` | `0xF2` | Префикс debug-подкоманд (ниже) |

- Debug-подкоманды, нужные на Этапе 1: `ENTER(SWD)`, `EXIT`, `SWD_SET_FREQ` (APIv2) / `SET_COM_FREQ` (APIv3), `READ_IDCODES`, `READMEM_32BIT`, `WRITEMEM_32BIT`, `READMEM_8BIT`, `READ_DEBUG_REG`/`WRITE_DEBUG_REG`, `FORCEDEBUG`, `RUNCORE`, `RESETSYS`, `READREG`, `READALLREGS`, `GETLASTRWSTATUS(2)`, `DRIVE_NRST`.
- **Feature gating по версии прошивки.** После `GET_VERSION` библиотека строит набор `ProbeCapabilities` (APIv1/v2/v3, 16-битный доступ к памяти, multi-AP, `GETLASTRWSTATUS2`, макс. размер пакета чтения и т.д.). Команды, не поддерживаемые прошивкой, не отправляются — вместо этого используется обходной путь или `Error::Unsupported`. Пороги версий берутся из `stlink`/OpenOCD.
- **Инициализация:** `GET_CURRENT_MODE` → если ST-Link в DFU-режиме, выполнить `DFU_EXIT`; если в Debug — `EXIT` → затем `ENTER(SWD)`.
- **Обработка статусов:** каждый ответ со статусом ≠ `0x80` (OK) маппится в типизированную ошибку (`SwdFault`, `ApWait`, `DpWait`, ...). Для `WAIT` — ограниченное количество ретраев.
- **Logging/трассировка (обязательно с первого дня):** через `tracing`. На уровне `trace` каждый пакет пишется как `→ <hex> (<имя команды>)` / `← <hex> (<расшифровка>)` с длительностью. В CLI уровень задаётся флагами `-v`, `-vv`, `-vvv`.
- **Acceptance criteria:**
  - Версия выводится в формате `st-info`: `V2J37S7` (V2) или `V3J7M3...` (V3) и совпадает с `st-info --probe`.
  - Напряжение таргета отображается с точностью до 0.01 В (≈3.3 В на стенде).
  - Повторный запуск CLI сразу после предыдущего (в т.ч. прерванного по Ctrl+C) работает без переподключения USB.

### F3. ARM debug-уровень (SWD / ADIv5 / Cortex-M)

- Чтение/запись памяти 8/32 бит (16 бит — если поддерживается прошивкой) с автоматической разбивкой на пакеты и обработкой выравнивания.
- Работа с ядром: halt, run, reset (system reset и hardware reset через NRST), чтение регистров ядра R0–R12, SP, LR, PC, xPSR, MSP, PSP (через `READREG` или через DCRSR/DCRDR).
- Идентификация ядра по `CPUID` (`0xE000ED00`): Cortex-M0/M0+/M3/M4/M7/M33.
- **Connect under reset** (опционально): удержание NRST при `ENTER(SWD)` — для прошивок, отключающих SWD или уводящих чип в low-power.
- **Acceptance criteria:**
  - Для H743: `CPUID` = `0x411FC271` (Cortex-M7 r1p1) или иная ревизия M7.
  - Регистры ядра читаются после halt; значения PC/SP адекватны (SP ∈ RAM, PC ∈ flash/RAM).
  - После `stlink-cli regs` ядро возвращается в исходное состояние (если было запущено — продолжает работу).

### F4. Идентификация чипа (аналог `st-info --probe`)

- Определение `chip_id` чтением DBGMCU_IDCODE по адресу, зависящему от семейства. Алгоритм — перебор по таблице:

```
по CPUID определить ядро
для каждого адреса из списка кандидатов для этого ядра:
    прочитать u32; dev_id = значение & 0xFFF
    если dev_id есть в таблице чипов → найдено
иначе → Error::UnknownChip(raw)
```

  Типичные адреса: `0xE0042000` (F1/F2/F3/F4/F7/L1/L4/G4/WB…), `0x40015800` (F0/G0/L0/C0), `0x5C001000` (H7), `0xE0044000` (U5/H5 и новее — уточнить по таблице).
- **Таблица чипов** — данные (не код): `chip_id`, `dev_type` (строка как в `st-info`), семейство, адрес регистра flash size, адрес UID, размер SRAM, раскладка flash (банки/секторы/страницы), адрес system bootloader. Источник — `config/chips/*.chip` из проекта `stlink` (BSD-3, можно заимствовать с указанием авторства).
- Вывод `stlink-cli probe` (формат как у `st-info --probe`, плюс доп. поля):

```
Found 1 stlink programmers
  version:    V2J37S7
  serial:     ...
  voltage:    3.28 V
  flash:      2097152 (pagesize: 131072)
  sram:       ...
  chipid:     0x450
  dev-type:   STM32H74x_H75x
  core:       Cortex-M7 r1p1 (CPUID 0x411FC271)
  uid:        ...
```

- **Acceptance criteria:**
  - На H743 поля `version`, `serial`, `flash`, `chipid`, `dev-type` совпадают с `st-info --probe`.
  - Неизвестный чип не ломает вывод: показывается `chipid` и `dev-type: unknown`.
  - Таблица покрывает минимум: F0, F1, F2, F3, F4 (вкл. F427/F429), F7, G0, G4, H7, L0, L1, L4, WB.

### F5. Чтение flash в `.bin` (dump)

- Чтение произвольного диапазона памяти в файл; по умолчанию — весь flash.
- Прогресс через callback (`bytes_done`, `bytes_total`).
- Учёт кэша Cortex-M7 (H7/F7): чтение идёт по шине AHB-AP, а не через ядро — проблем нет, но при верификации после записи — инвалидировать D-cache, если ядро запущено.
- Предупреждение, если включена защита RDP (чтение вернёт ошибку/нули).
- **Acceptance criteria:** дамп H743 побайтно совпадает с `st-flash read` того же диапазона.

### F6. Запись flash из `.bin` через ST-Link

- Вход: `.bin`-файл + базовый адрес (по умолчанию — начало flash, `0x08000000`).
- Шаги: валидация (файл помещается во flash, адрес выровнен) → halt → unlock → стирание затрагиваемых секторов/страниц → программирование → lock → верификация (чтение + сравнение или CRC32) → reset/run (опционально).
- Драйверы flash — по семействам, через общий трейт `FlashDriver`. Порядок реализации: **H7** (стенд), **F4** (F427), затем F1, L4, G4, G0, F0, F7.
- Особенности H7: 2 банка, секторы 128 КБ, программирование словами 256 бит (32 байта); буфер дополняется `0xFF` до кратности 32.
- Два режима записи (решить при реализации): (a) прямая запись регистров flash по SWD — проще, медленнее; (b) загрузка flash loader в RAM таргета — быстрее (как в `stlink`/OpenOCD).
- Прогресс по стадиям: `Erasing`, `Programming`, `Verifying`; отмена между блоками.
- **Acceptance criteria:**
  - Запись 64 КБ на H743 + верификация проходит; программа на плате запускается после reset.
  - Обрыв USB во время записи → понятная ошибка, повторная запись после переподключения проходит успешно.
  - Файл больше flash или невыровненный адрес → ошибка до начала стирания.

### F7. USB DFU (DfuSe) — без ST-Link

- Обнаружение устройств STM32 system bootloader: `0483:df11`.
- Разбор строк интерфейсов/alt-settings вида `@Internal Flash  /0x08000000/04*016Kg,01*064Kg,07*128Kg` → раскладка памяти (адрес, количество и размер секторов, атрибуты r/e/w).
- Операции DfuSe: `SET_ADDRESS_POINTER`, `ERASE` (по секторам), `DNLOAD` блоками (`wTransferSize` из functional descriptor), `UPLOAD` для верификации, `GETSTATUS`/`CLRSTATUS`/`ABORT`, выход из DFU («leave», запуск прошивки).
- Горячее подключение: устройство появляется после reset с BOOT0=1.
- Первый целевой чип — **STM32F427** (секторы 4×16 КБ, 1×64 КБ, 7×128 КБ на банк).
- **Acceptance criteria:**
  - `stlink-cli dfu list` показывает F427 в DFU-режиме с разобранной раскладкой памяти.
  - Запись `.bin` + верификация через UPLOAD совпадает; после «leave» прошивка запускается.
  - Логика DfuSe (разбор дескрипторов, состояния) покрыта unit-тестами без железа.

### F8. GUI (Этап 2, концептуально)

- Главный экран: список устройств (ST-Link и DFU) с горячим подключением → карточка выбранного: программатор, напряжение, чип, flash/SRAM, UID.
- Действия: «Прочитать в файл», «Записать из файла» (drag-and-drop `.bin`, адрес), «Стереть», «Reset», «Сверить».
- Прогресс-бар по стадиям, отмена, журнал операций (с выгрузкой лога `trace` для багрепортов).
- Подтверждение перед разрушительными операциями (стирание, запись).
- Детали — в отдельном PRD Этапа 2.

---

## 5. Технический стек

| Слой | Технология | Почему |
|---|---|---|
| USB | [`nusb`](https://docs.rs/nusb) 0.2.x | Чистый Rust, без libusb; async; hot-plug; используется в probe-rs |
| Логирование | [`tracing`](https://docs.rs/tracing) + `tracing-subscriber` | Структурированные логи, уровни, hex-трейс пакетов |
| Ошибки | [`thiserror`](https://docs.rs/thiserror) (библиотека), `anyhow` (CLI) | Типизированные ошибки для GUI |
| CLI | [`clap`](https://docs.rs/clap) | Подкоманды `list`, `probe`, `regs`, `read`, `write`, `dfu ...` |
| Async-runtime | Собственный рабочий поток на устройство + `futures-lite::block_on` для nusb; в Tauri — `tauri::async_runtime` | Не тащить tokio в библиотеку |
| GUI | Tauri 2 + React 19 + TypeScript + Vite | Уже настроено в репозитории |
| Справочники | [stlink](https://github.com/stlink-org/stlink) (BSD-3), [OpenOCD](https://openocd.org) (GPL-2.0, только как знание протокола), [probe-rs](https://github.com/probe-rs/probe-rs) (MIT/Apache) | Протокол, обходы для клонов, таблицы чипов |
| Документация ST | RM0433 (H7), RM0090 (F4), AN3156 (USB DFU), AN2606 (system bootloader), UM1075 (DfuSe) | Регистры flash, DfuSe, bootloader |

**Структура репозитория (Cargo workspace):**

```
stlink-rust-ui/
├── Cargo.toml          # [workspace]: stlink, stlink-cli, src-tauri
├── stlink/             # библиотека
│   └── src/ usb/ probe/ arm/ chips/ flash/ dfu/ error.rs
├── stlink-cli/         # консольный стенд
├── src-tauri/          # Tauri backend (Этап 2)
├── src/                # React frontend (Этап 2)
└── docs/               # PRD и заметки
```

**Лицензирование:** код OpenOCD не копируется (GPL). Данные/код из `stlink` и `probe-rs` — допустимо с указанием источника в `NOTICE`.

---

## 6. Концептуальная модель данных

| Сущность | Поля (тип) | Примечание |
|---|---|---|
| `ProbeDescriptor` | `vid: u16`, `pid: u16`, `model: ProbeModel {V2, V2_1, V3}`, `serial: String`, `bus_id: String`, `device_address: u8` | Результат discovery, без открытия устройства |
| `ProbeVersion` | `stlink: u8`, `jtag: u8`, `swim: Option<u8>`, `msd: Option<u8>`, `bridge: Option<u8>`, `api: ApiVersion {V1, V2, V3}` | `Display` → `V2J37S7` |
| `ProbeCapabilities` | набор флагов: `mem_16bit`, `multi_ap`, `get_last_rw_status2`, `set_freq`, `max_read_size: usize`, … | Выводится из `ProbeVersion` |
| `ProbeInfo` | `descriptor: ProbeDescriptor`, `version: ProbeVersion`, `target_voltage: Option<f32>` | |
| `ChipDescriptor` | `chip_id: u16`, `dev_type: &str`, `family: Family`, `idcode_addr: u32`, `flash_size_addr: u32`, `uid_addr: Option<u32>`, `sram_size: u32`, `flash_base: u32`, `flash_layout: FlashLayout`, `bootloader_addr: Option<u32>` | Статическая таблица |
| `FlashLayout` | `banks: Vec<FlashBank { base: u32, sectors: Vec<SectorRun { count: u32, size: u32 }> }>`, `write_granularity: u32`, `erased_value: u8` | Общая для ST-Link и DFU |
| `CoreInfo` | `cpuid: u32`, `core: CoreType {M0, M0Plus, M3, M4, M7, M33}`, `revision: (u8, u8)` | |
| `TargetInfo` | `chip: Option<&ChipDescriptor>`, `chip_id_raw: u32`, `core: CoreInfo`, `flash_size: u32`, `uid: Option<[u8; 12]>` | Результат `probe` |
| `CoreRegisters` | `r: [u32; 13]`, `sp`, `lr`, `pc`, `xpsr`, `msp`, `psp` (`u32`) | |
| `DfuDevice` | `descriptor` (как у Probe), `alt_settings: Vec<DfuAlt { alt: u8, name: String, layout: FlashLayout, attrs }>`, `transfer_size: u16`, `dfu_version: u16` | |
| `FlashJob` | `source: PathBuf`, `base_address: u32`, `size: u32`, `crc32: u32`, `verify: bool`, `reset_after: bool` | |
| `Progress` | `stage: Stage {Erasing, Programming, Verifying, Reading}`, `done: u64`, `total: u64` | Callback/событие в GUI |
| `Error` | `NotFound`, `AccessDenied`, `Disconnected`, `Timeout`, `Usb(..)`, `ProbeStatus(u8)`, `SwdFault`, `TargetNotConnected`, `UnknownChip(u32)`, `Unsupported(&str)`, `FlashProtected`, `Verify { addr: u32 }`, `Dfu(..)` | Типизированные, без `panic!` |

**Хранимые данные GUI (Этап 2):** настройки (частота SWD, verify по умолчанию, connect under reset), история файлов (путь, последний адрес) — JSON в каталоге приложения через Tauri store.

---

## 7. Принципы UI (Этап 2, кратко)

- Минимализм: один главный экран, без вложенных меню.
- Состояние устройства всегда видно; горячее подключение без кнопки «Обновить».
- Разрушительные действия — с подтверждением и явным указанием, что будет стёрто.
- Тёмная и светлая тема (следовать системной).
- Ошибки — человеческим языком + «Подробнее» (технические детали из лога).

---

## 8. Безопасность и надёжность

- Нет сети, нет аккаунтов — поверхность атаки минимальна. Приложение не отправляет данные наружу.
- **Никогда** не менять RDP/Option Bytes в v1. RDP Level 2 необратим — запрещён на уровне библиотеки.
- Mass erase — только по явному запросу.
- Валидация файла и адреса до любых разрушительных действий.
- Все USB-операции с таймаутами; при отключении — освобождение ресурсов, без паник (`panic = "abort"` в release делает паники фатальными).
- Однопоточный доступ к одному устройству: команды сериализуются через рабочий поток/актор устройства.

---

## 9. Этапы (milestones)

| # | Название | Содержание | Критерий готовности |
|---|---|---|---|
| **M0** | Каркас | Cargo workspace, crates `stlink` + `stlink-cli`, `tracing`, ошибки, CI-проверка `cargo check` для macOS | `stlink-cli --help` работает |
| **M1** | **Discovery + probe + регистры** ← *первая задача* | F1, F2 (базовые команды), F3 (память, halt, регистры), F4 (таблица чипов) | `stlink-cli probe` на H743 совпадает с `st-info --probe`; `stlink-cli regs` выводит регистры ядра |
| M2 | Чтение flash | F5, прогресс, частота SWD | Дамп совпадает с `st-flash read` |
| M3 | Запись flash (ST-Link) | F6: H7, затем F4 и др. | Запись + verify на H743 |
| M4 | DFU | F7 на F427 | Запись + verify через DfuSe |
| M5 | GUI | F8: Tauri-команды и события, React-экраны | Полный сценарий чтения/записи из GUI |
| M6 | Расширения | `.hex`, Windows, Linux | См. §11 |

---

## 10. Риски и решения

| Риск | Решение |
|---|---|
| Клоны отвечают нестандартно / старая прошивка | Feature gating по версии; trace-лог; сверка с `st-info`/OpenOCD |
| H7: несколько AP, старый V2 умеет только AP0 | Для идентификации и flash достаточно AP0 (AHB-AP D1); multi-AP — только при поддержке прошивкой |
| H7: DBGMCU по нестандартному адресу | Табличный перебор адресов IDCODE |
| Таргет со «спящей»/отключённой SWD-прошивкой | Connect under reset (требует подключённого NRST) |
| Кэш Cortex-M7 при верификации | Чтение через AHB-AP; при необходимости — halt и инвалидация |
| Нет железа в среде разработки | `cargo check` под macOS-target в VM; unit-тесты без железа; trace-логи от пользователя |
| DfuSe: нюансы F4 bootloader (статусы, задержки стирания) | Поллинг `GETSTATUS` с `bwPollTimeout`; тесты на эмуляции ответов |
| Лицензия GPL OpenOCD | Только знания протокола, код не копируется |

---

## 11. Будущее расширение

- **Windows:** WinUSB-драйвер для ST-Link и `0483:df11` (Zadig или WCID-инструкции в приложении); подпись приложения.
- **Linux:** udev-правила; AppImage/deb.
- **`.hex`** (Intel HEX) — с несколькими сегментами.
- Option Bytes (чтение → редактирование с защитами).
- Multi-AP / доступ к D3 домену H7; SWO-вывод; RTT-консоль.
- Пакетная прошивка нескольких плат, профили проектов.
- ST-Link V3 bridge-функции (UART/I2C/SPI).

---

## 12. Открытые вопросы

1. Режим записи flash через ST-Link: прямой (через регистры) или flash loader в RAM — решить на M3 по скорости.
2. Нужен ли выход в JSON у `stlink-cli` (для тестов и интеграций)?
3. Минимальная версия macOS.
4. Нужна ли поддержка нескольких одновременно подключённых ST-Link в GUI v1.
