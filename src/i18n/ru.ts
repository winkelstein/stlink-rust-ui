// Russian strings. Keys follow the `T` dictionary of docs/design/Main.dc.html;
// strings that hold sample values there are functions here. `en.ts` has the
// same shape (`Dict`).

import { plural } from "../domain/format";

const sectors = (n: number) => `${n} ${plural(n, ["сектор", "сектора", "секторов"])}`;

export const ru = {
  lang: "ru" as "ru" | "en",

  // titlebar
  steps: "Шаги",
  stepDevice: "Устройство",
  stepParams: "Параметры",
  stepRun: "Выполнение",
  log: "Журнал",
  settings: "Настройки",

  // empty
  emptyTitle: "Подключите программатор",
  emptyText: "ST-Link по USB или плату в режиме DFU (BOOT0 = 1). Устройство появится здесь автоматически.",
  waiting: "Ожидание устройства…",

  // released
  relTitle: "Программатор отключён",
  connect: "Подключиться",
  connecting: "Подключение…",
  chipReleased: (model: string) => `${model} · отключён`,
  busyTitle: "Программатор занят",
  busyText: (model: string) =>
    `${model} занят другой программой (VS Code, OpenOCD, st-flash, STM32CubeProgrammer). Закройте её или отключите в ней программатор и нажмите «Повторить».`,
  retry: "Повторить",
  driverTitle: "Нужен драйвер WinUSB",
  driverText: (model: string) =>
    `${model} найден, но Windows подключила его через драйвер, с которым приложение не работает. Установите драйвер ST-Link от ST (он ставится и вместе со STM32CubeProgrammer), переподключите программатор и нажмите «Повторить».`,
  driverStLink: "Драйвер ST-Link (STSW-LINK009)",
  driverZadig: "WinUSB через Zadig (для DFU)",

  // device
  quick: "Быстрая запись",
  changedAgo: (ago: string) => `изменён ${ago}`,
  watching: "слежу за файлом",
  noFile: "нет файла",
  // TODO(design): quick write card without a remembered file.
  quickEmpty: "Запишите прошивку один раз — файл и адрес запомнятся для этого МК",
  onChange: "При изменении файла",
  flashBtn: "Записать",
  programmer: "Программатор",
  connected: "Подключён",
  firmware: "Прошивка",
  serial: "Серийный номер",
  voltage: "Напряжение",
  swd: "Частота SWD",
  mcu: "Микроконтроллер",
  core: "Ядро",
  flashVal: (size: string, banks: number, perBank: number) =>
    banks > 1 ? `${size} · ${banks} банка по ${perBank} секторов` : `${size} · ${sectors(perBank)}`,
  rdp: "Защита чтения",
  rdpVal: { Level0: "RDP 0 · нет", Level1: "RDP 1 · чтение запрещено", Level2: "RDP 2 · отладка отключена" },
  other: "Другие действия",
  tileWrite: "Записать прошивку",
  tileWriteSub: ".bin → flash, с проверкой",
  tileRead: "Прочитать в файл",
  tileReadSub: "flash → .bin",
  tileErase: "Стереть всё",
  tileEraseSub: (size: string) => `весь flash, ${size}`,
  noFlashDriver: "Запись и стирание для этого МК пока не поддерживаются",
  chipConnected: (model: string) => `${model} · подключён`,
  release: "Отключить",
  releaseHint: "Отключиться от программатора, чтобы им могли пользоваться VS Code, OpenOCD или st-flash",
  reboot: "Перезагрузить МК",
  rebootHint: "Аппаратный сброс и запуск прошивки",
  // TODO(design): MCU card while connecting / after a failed connect.
  mcuReading: "Чтение IDCODE, CPUID и раскладки flash",
  mcuNoLink: "Нет связи с МК",
  mcuCheckWiring: "Проверьте подключение SWDIO/SWCLK/GND и питание платы",
  unknownChip: "Неизвестный МК",
  unknownCore: "Неизвестное ядро",

  // erase all
  eraseCfTitle: "Стереть весь flash?",
  eraseCf1: (banks: number): string => (banks > 1 ? "Оба банка" : "Весь flash"),
  eraseCf2: "будут стёрты полностью: прошивка, загрузчик и данные во flash. Отменить это нельзя.",
  cancel: "Отмена",
  eraseAllBtn: "Стереть всё",

  // write form
  writeTitle: "Запись прошивки",
  fileMeta: (size: string, bytes: string, time: string) => `${size} (${bytes} байт) · изменён в ${time}`,
  noFileChosen: "Файл не выбран",
  chooseBin: "Выберите .bin для записи",
  choose: "Выбрать…",
  dropHint: "Или перетащите .bin в окно",
  addr: "Адрес записи",
  verify: "Проверять после записи",
  resetAfter: "Перезагружать МК после записи",
  watch: "Перезаписывать при изменении файла",
  map: (size: string) => `Карта flash · ${size}`,
  bank1: "Банк 1",
  bank2: "Банк 2",
  bankN: (n: number) => `Банк ${n}`,
  sectorTitle: (bank: number, index: number, addr: string) => `Банк ${bank}, сектор ${index} · ${addr}`,
  willErase: (what: string) => `Будут стёрты: ${what}`,
  fw: "Прошивка",
  unchanged: "Не изменится",
  estimate: (total: number, erase: string, write: string, verify: string | null) =>
    `Примерно ${total} с: стирание ${erase} · запись ${write}${verify ? ` · проверка ${verify}` : ""}`,
  estimateNoFile: "Выберите файл, чтобы увидеть, что будет стёрто",
  writeUnsupported: "Запись для этого МК пока не поддерживается.",
  back: "Назад",
  eraseWrite: "Стереть и записать",

  // sectors: `сектора 0–1, банк 1`, `сектор 7 банка 1 и 0 банка 2`
  runs: (runs: { span: string; bank: number; many: boolean }[], many: boolean, long: boolean) => {
    const word = many ? "сектора" : "сектор";
    if (runs.length === 1 && !long) return `${word} ${runs[0].span}, банк ${runs[0].bank}`;
    return `${word} ${runs.map((r) => `${r.span} банка ${r.bank}`).join(" и ")}`;
  },

  // address / range validation
  addrEnter: "Введите адрес: 0x08000000 или десятичное число",
  addrFlashStart: (bank: number) => `Начало flash, банк ${bank}`,
  addrBankStart: (bank: number) => `Начало банка ${bank}`,
  addrSector: (index: number, bank: number) => `Сектор ${index}, банк ${bank}`,
  addrAligned: (where: string) => `${where} · выровнено по сектору`,
  addrUnaligned: (where: string, sector: string) =>
    `${where} · не с начала сектора: сектор с ${sector} будет стёрт целиком`,
  planEmpty: "Файл пустой — записывать нечего",
  planGeometry: "Неизвестная раскладка flash",
  planOutside: (range: string) => `Адрес вне flash (${range})`,
  planUnaligned: (n: number) => `Адрес должен быть кратен ${n} байтам`,
  planDoesNotFit: (left: string, need: string) => `Файл не помещается: до конца flash ${left}, нужно ${need}`,
  readEnterStart: "Введите начальный адрес: 0x08000000",
  readEnterSize: "Введите размер: 0x40000, 256K или 262144",
  readTooBig: (max: string) => `Не больше ${max} от этого адреса до конца flash`,
  readMax: (max: string) => `До конца flash: ${max}`,
  readToEnd: "до конца flash",
  readUnknown: "Размер flash неизвестен — чтение недоступно",

  // confirm
  cfTitle: (n: number) => `Стереть ${sectors(n)} и записать прошивку?`,
  cf2: (n: number) => `${n === 1 ? "будет стёрт" : "будут стёрты"}. Остальная память не изменится.`,

  // progress
  writing: (name: string) => `Запись ${name}`,
  erasing: "Полное стирание",
  dontUnplug: "Не отключайте программатор",
  abort: "Отменить",
  stErase: "Стирание",
  stProgram: "Программирование",
  stVerify: "Проверка",
  stWaiting: "ожидание",
  stOff: "выключено",
  stMatch: "совпадает",
  sectorOf: (i: number, n: number) => `сектор ${i} из ${n}`,
  sectorsN: sectors,
  kbOf: (pct: number, done: number, total: number, rate: string) => `${pct} % · ${done} из ${total} КБ · ${rate}`,
  left: (s: string) => `осталось ${s}`,
  progressLabel: "Общий прогресс",

  // write done
  doneTitle: "Прошивка записана и проверена",
  doneTitleNoVerify: "Прошивка записана",
  doneSub: (name: string, size: string, time: string, restarted: boolean) =>
    `${name} · ${size} за ${time} · ${restarted ? "МК перезагружен" : "ядро остановлено"}`,
  region: "Область",
  erased: "Стёрто",
  erasedVal: (n: number, size: string) => `${sectors(n)} · ${size}`,
  toDevice: "К устройству",
  again: "Записать снова",

  // erase done
  eraseDoneTitle: "Flash стёрт",
  eraseDoneSub: (size: string, time: string) => `${size} за ${time} · все байты 0xFF`,
  banks: "Банки",
  banksVal: (n: number) => (n > 1 ? Array.from({ length: n }, (_, i) => i + 1).join(n === 2 ? " и " : ", ") : "1"),
  mcuState: "МК",
  mcuHalted: "остановлен, прошивки нет",

  // read
  readTitle: "Чтение в файл",
  what: "Что прочитать",
  rangeAll: "Весь flash",
  rangeCustom: "Диапазон",
  start: "Начальный адрес",
  size: "Размер",
  change: "Изменить…",
  readEstimate: (s: number, size: string, rate: string) => `Примерно ${s} с · ${size} при ${rate}`,
  enterRange: "Укажите диапазон",
  readBtn: "Прочитать",
  readingTitle: "Чтение flash",
  ofMb: (done: string, total: string) => `${done} из ${total}`,
  rateLeft: (rate: string, s: number) => `${rate} · осталось ${s} с`,
  toFile: "В файл",
  coreRuns: "ядро продолжает работать",
  saved: "Файл сохранён",
  sizeVal: (size: string, bytes: string) => `${size} · ${bytes} байт`,
  time: "Время",
  finder: "Показать в Finder",
  finderWin: "Показать в проводнике",
  finderLinux: "Показать в папке",
  done: "Готово",

  // settings
  general: "Общие",
  language: "Язык",
  appearance: "Оформление",
  flashing: "Запись",
  alwaysConfirm: "Всегда спрашивать перед стиранием",
  alwaysConfirmHint: "Иначе быстрая запись не спрашивает, если стираются те же сектора",
  underReset: "Подключаться со сбросом",
  underResetHint: "Для прошивок, которые отключают SWD или засыпают",
  keepDebug: "Держать отладку в low-power",
  keepDebugHint: "Пишет DBGMCU_CR, действует до выключения питания",
  verbose: "Подробный журнал USB",
  verboseHint: "Каждый пакет в hex. Нужен для отчётов об ошибках",
  appliesOnConnect: "Параметры программатора применяются при следующем подключении",
  sys: "Системный",
  sysTheme: "Системное",
  light: "Светлое",
  darkT: "Тёмное",

  // log
  filter: "Фильтр",
  copy: "Копировать",
  copied: "Скопировано",
  saveLog: "Сохранить…",
  clear: "Очистить",
  close: "Закрыть",
  fAll: "Все",
  fErr: "Ошибки",
  fUsb: "USB",
  entries: (n: number) => `${n} ${plural(n, ["запись", "записи", "записей"])}`,
  logEmpty: "Записей нет",
  usbHint: "USB-пакеты пишутся, только если включён «Подробный журнал USB»",

  // errors (`BackendError.kind`; `message` goes to «Подробнее» and the log)
  // TODO(design): error banners are not in the mockups.
  details: "Подробнее",
  hide: "Скрыть",
  partial: "Flash мог остаться частично записан, ядро остановлено.",
  partialErase: "Flash мог остаться частично стёрт, ядро остановлено.",
  errDisconnected: "Программатор отключён",
  errDisconnectedText: "Связь с программатором потеряна. Подключите его снова.",
  errDisconnectedWrite: "Связь потеряна во время записи.",
  errDisconnectedWriteTail: "Подключите программатор и повторите запись.",
  errCore: "Ядро МК не отвечает",
  errCoreText:
    "Прошивка, вероятно, уводит МК в low-power с выключенной отладкой. Подключитесь со сбросом или включите отладку в low-power (DBGMCU_CR сохранится до выключения питания).",
  errProtected: "Flash защищена",
  errProtectedText: "Включена защита чтения (RDP). Снять её из приложения нельзя — используйте STM32CubeProgrammer.",
  errBusy: "Программатор занят",
  errBusyText: "ST-Link используется другой программой (OpenOCD, st-util, STM32CubeProgrammer). Закройте её и повторите.",
  errDriver: "Нужен драйвер WinUSB",
  errDriverText: "Установите драйвер ST-Link от ST и переподключите программатор.",
  errUnsupported: "Не поддерживается",
  errUnsupportedText: "Эта операция не поддерживается для этого программатора или МК.",
  errOther: "Ошибка",
  errWrite: "Ошибка записи",
  errRead: "Ошибка чтения",
  errErase: "Ошибка стирания",
  errOtherText: "Операция не выполнена.",
  onlyBin: (name: string) => `«${name}»: поддерживаются только файлы .bin`,
  cancelledWrite: "Запись отменена",
  cancelledRead: "Чтение отменено",
  cancelledErase: "Стирание отменено",
  notSaved: "Файл не сохранён.",
  cancelled: "Отменено",
  actRetryWrite: "Повторить запись",
  actRetry: "Повторить",
  actUnderReset: "Подключение со сбросом",
  actKeepDebug: "Держать отладку в low-power",
};

export type Dict = typeof ru;
