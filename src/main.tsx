import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import Pill from "./Pill";
import "./styles/quiet.css";
import "./styles/app.css";

async function start() {
  const inApp = "__TAURI_INTERNALS__" in window;

  // In a plain browser during development there is no Rust backend, so fall
  // back to canned answers. The real app always has __TAURI_INTERNALS__.
  if (import.meta.env.DEV && !inApp) {
    (await import("./dev-mock")).installMock();
  }

  // One bundle serves both windows. In the app the window's label says which
  // one this is; in the browser preview, open "/#pill" to see the pill.
  const isPill = inApp
    ? getCurrentWindow().label === "pill"
    : window.location.hash === "#pill";
  document.documentElement.classList.add(isPill ? "pill-window" : "main-window");

  // Quiet in light, Graphite in dark. The page's colour scheme follows its
  // window, which follows the Mac unless Appearance in About fixes it.
  const dark = window.matchMedia("(prefers-color-scheme: dark)");
  const follow = () => {
    document.documentElement.classList.toggle("q-dark", dark.matches);
    document.documentElement.classList.toggle("q-light", !dark.matches);
  };
  follow();
  dark.addEventListener("change", follow);

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>{isPill ? <Pill /> : <App />}</React.StrictMode>,
  );
}

start();
