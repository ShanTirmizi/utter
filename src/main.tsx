import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Pill from "./Pill";

// One bundle serves both windows; the status pill is opened at "#pill".
const isPill = window.location.hash === "#pill";
if (isPill) document.documentElement.classList.add("pill-window");

async function start() {
  // In a plain browser during development there is no Rust backend, so fall
  // back to canned answers. The real app always has __TAURI_INTERNALS__.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./dev-mock")).installMock();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{isPill ? <Pill /> : <App />}</React.StrictMode>,
  );
}

start();
