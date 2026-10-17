import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Segmented } from "../components/Segmented";
import { Sheet } from "../components/Sheet";
import type { LogEntry } from "../backend/types";
import { fmtDate, fmtLogTime } from "../domain/format";
import { useT } from "../i18n";
import { useApp } from "./shared";

type Filter = "all" | "err" | "usb";

const CAPACITY = 5000;

function visible(e: LogEntry, filter: Filter, verbose: boolean): boolean {
  if (filter === "err") return e.level === "WARN" || e.level === "ERROR";
  if (filter === "usb") return e.level === "TRACE";
  return e.level !== "TRACE" || verbose;
}

function line(e: LogEntry): string {
  return `${fmtLogTime(e.ts)} ${e.level.padEnd(5)} ${e.target}: ${e.message}`;
}

/** «Журнал»: the backend ring buffer, live; filter, copy, save, clear. */
export function LogSheet() {
  const { s, ctl } = useApp();
  const { t } = useT();
  const close = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [entries, setEntries] = useState<LogEntry[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [copied, setCopied] = useState(false);
  /** Follow new lines until the user scrolls up. */
  const stick = useRef(true);
  const verbose = s.settings.verboseUsb;
  const backend = ctl.backend;

  useEffect(() => {
    let alive = true;
    const pending: LogEntry[] = [];
    let loaded = false;
    const unsub = backend.watchLog((batch) => {
      if (!loaded) pending.push(...batch);
      else setEntries((prev) => [...prev, ...batch].slice(-CAPACITY));
    });
    void backend.logEntries().then((all) => {
      if (!alive) return;
      loaded = true;
      const last = all.length ? all[all.length - 1].seq : 0;
      setEntries([...all, ...pending.filter((e) => e.seq > last)].slice(-CAPACITY));
    });
    return () => {
      alive = false;
      unsub();
    };
  }, [backend]);

  const rows = useMemo(() => entries.filter((e) => visible(e, filter, verbose)), [entries, filter, verbose]);

  useLayoutEffect(() => {
    const el = list.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [rows]);

  const text = () => rows.map(line).join("\n") + "\n";
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text());
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      console.error("clipboard", e);
    }
  };
  const save = () => void backend.saveLog(text(), `stlink-ui-${fmtDate(Date.now())}.log`).catch((e: unknown) => console.error(e));
  const clear = () =>
    void backend.clearLog().then(() => {
      setEntries([]);
      stick.current = true;
    });
  const onClose = () => ctl.store.dispatch({ type: "overlayClosed" });

  return (
    <Sheet labelledBy="log-title" onClose={onClose} initialFocus={close} style={{ width: 860, height: 520, padding: "16px 0 0" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12, padding: "0 16px 12px", borderBottom: "1px solid var(--sep)" }}>
        <h3 id="log-title" style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
          {t.log}
        </h3>
        <div style={{ width: 260 }}>
          <Segmented<Filter>
            label={t.filter}
            value={filter}
            options={[
              { value: "all", label: t.fAll },
              { value: "err", label: t.fErr },
              { value: "usb", label: t.fUsb },
            ]}
            onChange={(v) => {
              stick.current = true;
              setFilter(v);
            }}
          />
        </div>
        <div style={{ flex: 1 }} />
        <Button className="btn-sm" disabled={rows.length === 0} onClick={() => void copy()}>
          {copied ? t.copied : t.copy}
        </Button>
        <Button className="btn-sm" disabled={rows.length === 0} onClick={save}>
          {t.saveLog}
        </Button>
        <Button className="btn-sm" disabled={entries.length === 0} onClick={clear}>
          {t.clear}
        </Button>
      </div>
      <div
        ref={list}
        className="mono"
        role="log"
        aria-live="off"
        style={{ flex: 1, overflow: "auto", background: "var(--logBg)", padding: "6px 0", minHeight: 0 }}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
      >
        {rows.length === 0 ? (
          <div style={{ padding: "12px 14px", fontSize: 12, color: "var(--text2)" }}>
            {filter === "usb" && !verbose ? t.usbHint : t.logEmpty}
          </div>
        ) : (
          rows.map((e) => (
            <div key={e.seq} className={e.level === "ERROR" ? "logrow err" : "logrow"} title={e.target}>
              <span className="ts">{fmtLogTime(e.ts)}</span>
              <span className={`lvl ${e.level}`}>{e.level}</span>
              <span className="msg">{e.message}</span>
            </div>
          ))
        )}
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "10px 16px", borderTop: "1px solid var(--sep)" }}>
        <Checkbox
          style={{ fontSize: 12.5 }}
          checked={verbose}
          onChange={() => ctl.store.dispatch({ type: "settingsChanged", patch: { verboseUsb: !verbose } })}
        >
          {t.verbose}
        </Checkbox>
        <div style={{ flex: 1 }} />
        <span style={{ fontSize: 12, color: "var(--text2)" }}>{t.entries(rows.length)}</span>
        <Button primary ref={close} onClick={onClose}>
          {t.close}
        </Button>
      </div>
    </Sheet>
  );
}
