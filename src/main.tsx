import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

if (new URLSearchParams(window.location.search).get("view") === "widget") {
  document.body.classList.add("widget-body");
  document.documentElement.classList.add("widget-root");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
