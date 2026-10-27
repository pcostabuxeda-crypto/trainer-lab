import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import { useGames } from "./stores/games";
import { useLogs } from "./stores/logs";
import { useSession } from "./stores/session";
import { useSettings } from "./stores/settings";
import "./index.css";

void useLogs.getState().start();
void useSession.getState().start();
void useGames.getState().start();
void useSettings.getState().load();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
