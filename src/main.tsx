import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { createBackend, isTauri } from "./backend";
import { Controller } from "./state/controller";
import { initialState } from "./state/machine";
import { createStore } from "./state/store";
import "./styles/tokens.css";

const backend = createBackend();
const ctl = new Controller(backend, createStore(initialState));
// Outside React: StrictMode must not start hot-plug twice.
ctl.start();

if (isTauri()) document.body.classList.add("tauri");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App ctl={ctl} />
  </React.StrictMode>,
);
