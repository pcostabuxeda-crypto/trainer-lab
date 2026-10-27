import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import { useLogs } from "./stores/logs";
import { useSession } from "./stores/session";
import "./index.css";

void useLogs.getState().start();
void useSession.getState().start();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
