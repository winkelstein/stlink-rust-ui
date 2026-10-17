import { useRef } from "react";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Segmented } from "../components/Segmented";
import { Sheet } from "../components/Sheet";
import { SWD_FREQS, type AppSettings, type LangSetting, type ThemeSetting } from "../backend/types";
import { useT } from "../i18n";
import { LANGUAGE_NAME } from "../i18n/ru";
import { useApp } from "./shared";

/** «Settings» (titlebar button, ⌘,). Changes apply and persist right away. */
export function SettingsSheet() {
  const { s, ctl } = useApp();
  const { t, f } = useT();
  const done = useRef<HTMLButtonElement>(null);
  const st = s.settings;
  const set = (patch: Partial<AppSettings>) => ctl.store.dispatch({ type: "settingsChanged", patch });
  const close = () => ctl.store.dispatch({ type: "overlayClosed" });
  const toggle = (k: "verify" | "resetAfter" | "alwaysConfirm" | "underReset" | "keepDebugInLowPower" | "verboseUsb") =>
    set({ [k]: !st[k] });
  return (
    <Sheet labelledBy="set-title" onClose={close} initialFocus={done} style={{ width: 760, padding: "18px 22px 16px", gap: 14 }}>
      <h3 id="set-title" style={{ margin: 0, fontSize: 15, fontWeight: 600 }}>
        {t.settings}
      </h3>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 14, alignItems: "start" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="group">
            <div className="eyebrow">{t.general}</div>
            <div>
              <span className="label">{t.language}</span>
              <Segmented<LangSetting>
                label={t.language}
                value={st.lang}
                options={[
                  { value: "system", label: t.sys },
                  { value: "ru", label: LANGUAGE_NAME },
                  { value: "en", label: "English" },
                ]}
                onChange={(lang) => set({ lang })}
              />
            </div>
            <div>
              <span className="label">{t.appearance}</span>
              <Segmented<ThemeSetting>
                label={t.appearance}
                value={st.theme}
                options={[
                  { value: "system", label: t.sysTheme },
                  { value: "light", label: t.light },
                  { value: "dark", label: t.darkT },
                ]}
                onChange={(theme) => set({ theme })}
              />
            </div>
          </div>
          <div className="group">
            <div className="eyebrow">{t.flashing}</div>
            <Checkbox checked={st.verify} onChange={() => toggle("verify")}>
              {t.verify}
            </Checkbox>
            <Checkbox checked={st.resetAfter} onChange={() => toggle("resetAfter")}>
              {t.resetAfter}
            </Checkbox>
            <div>
              <Checkbox checked={st.alwaysConfirm} onChange={() => toggle("alwaysConfirm")}>
                {t.alwaysConfirm}
              </Checkbox>
              <div className="hint">{t.alwaysConfirmHint}</div>
            </div>
          </div>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="group">
            <div className="eyebrow">{t.programmer}</div>
            <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <label htmlFor="swd" style={{ fontSize: 13, flex: 1 }}>
                {t.swd}
              </label>
              <select
                id="swd"
                className="field"
                style={{ width: 130 }}
                value={st.swdKhz}
                onChange={(e) => set({ swdKhz: Number(e.currentTarget.value) })}
              >
                {SWD_FREQS.map((khz) => (
                  <option key={khz} value={khz}>
                    {f.freq(khz)}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <Checkbox checked={st.underReset} onChange={() => toggle("underReset")}>
                {t.underReset}
              </Checkbox>
              <div className="hint">{t.underResetHint}</div>
            </div>
            <div>
              <Checkbox checked={st.keepDebugInLowPower} onChange={() => toggle("keepDebugInLowPower")}>
                {t.keepDebug}
              </Checkbox>
              <div className="hint">{t.keepDebugHint}</div>
            </div>
            {s.session && <div style={{ fontSize: 11.5, color: "var(--text2)" }}>{t.appliesOnConnect}</div>}
          </div>
          <div className="group">
            <div className="eyebrow">{t.log}</div>
            <div>
              <Checkbox checked={st.verboseUsb} onChange={() => toggle("verboseUsb")}>
                {t.verbose}
              </Checkbox>
              <div className="hint">{t.verboseHint}</div>
            </div>
          </div>
        </div>
      </div>
      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <Button primary ref={done} onClick={close}>
          {t.done}
        </Button>
      </div>
    </Sheet>
  );
}
