import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/geist/400.css";
import "@fontsource/geist/500.css";
import "@fontsource/geist/600.css";
import "@fontsource/geist-mono/400.css";
import "@fontsource/geist-mono/500.css";
import "./styles.css";
import App from "./App";
import { useFiles } from "./files/store";
import { useLayout } from "./layout/store";
import { useProjects } from "./projects/store";
import { useSettings } from "./settings/store";
import { useTasks } from "./tasks/store";
import { useUi } from "./ui";

// Dev only: the stores on `window`, so an acceptance run can drive and
// inspect the app over WebView2's remote-debugging port.
if (import.meta.env.DEV) Object.assign(window, { veal: { useFiles, useLayout, useProjects, useSettings, useTasks, useUi } });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
