import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createBackend, isTauri } from "./backend";
import { Controller } from "./state/controller";
import { initialState } from "./state/machine";
import { createStore } from "./state/store";
import "./styles/tokens.css";
import { disableZoom } from "./noZoom";

if (isTauri()) document.body.classList.add("tauri");
disableZoom();

void createBackend().then(
  (backend) => {
    const ctl = new Controller(backend, createStore({ ...initialState, settings: backend.loadSettings() }));
    // Outside React: StrictMode must not start hot-plug twice.
    ctl.start();
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
      <React.StrictMode>
        <App ctl={ctl} />
      </React.StrictMode>,
    );
  },
  (e: unknown) => {
    console.error("cannot start the backend", e);
    const root = document.getElementById("root");
    if (root) root.textContent = `Cannot start: ${e instanceof Error ? e.message : String(e)}`;
  },
);
