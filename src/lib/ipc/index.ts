// Wrapper tipat de les comandes i esdeveniments de Rust. Els tipus de ./types els genera
// ts-rs (`npm run types`): no s'editen a mà.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ErrorPayload } from "./types/ErrorPayload";
import type { Locator } from "./types/Locator";
import type { LogEntry } from "./types/LogEntry";
import type { LogLevel } from "./types/LogLevel";
import type { MemoryReading } from "./types/MemoryReading";
import type { ProcessInfo } from "./types/ProcessInfo";
import type { SessionEvent } from "./types/SessionEvent";
import type { SessionInfo } from "./types/SessionInfo";
import type { ValueType } from "./types/ValueType";
import type { CatalogView } from "./types/CatalogView";
import type { CheatDef } from "./types/CheatDef";
import type { CheatInput } from "./types/CheatInput";
import type { CheatState } from "./types/CheatState";
import type { HotkeyFired } from "./types/HotkeyFired";
import type { Settings } from "./types/Settings";
import type { TrainerView } from "./types/TrainerView";
import type { GameEntry } from "./types/GameEntry";
import type { RunningGame } from "./types/RunningGame";
import type { PointerCandidate } from "./types/PointerCandidate";
import type { ScanFilter } from "./types/ScanFilter";
import type { ScanHit } from "./types/ScanHit";
import type { ScanSummary } from "./types/ScanSummary";

export type { Arch } from "./types/Arch";
export type { CheatView } from "./types/CheatView";
export type { VersionStatus } from "./types/VersionStatus";
export type { Store } from "./types/Store";
export type {
  CheatDef,
  PointerCandidate,
  ScanFilter,
  ScanHit,
  ScanSummary,
  GameEntry,
  RunningGame,
  CatalogView,
  CheatInput,
  CheatState,
  ErrorPayload,
  HotkeyFired,
  Locator,
  LogEntry,
  LogLevel,
  MemoryReading,
  ProcessInfo,
  SessionEvent,
  SessionInfo,
  Settings,
  TrainerView,
  ValueType,
};

export const listProcesses = () => invoke<ProcessInfo[]>("list_processes");

export const getLogs = (limit?: number, minLevel?: LogLevel) =>
  invoke<LogEntry[]>("get_logs", { limit, minLevel });

export const onLog = (cb: (e: LogEntry) => void): Promise<UnlistenFn> =>
  listen<LogEntry>("log", (ev) => cb(ev.payload));

// --- Sessió ---
export const attach = (pid: number, acknowledgeWarnings = false) =>
  invoke<SessionInfo>("attach", { pid, acknowledgeWarnings });
export const detach = () => invoke<boolean>("detach");
export const sessionInfo = () => invoke<SessionInfo | null>("session_info");
export const onSessionState = (cb: (e: SessionEvent) => void): Promise<UnlistenFn> =>
  listen<SessionEvent>("session-state", (ev) => cb(ev.payload));

// --- Trainers ---
export const listTrainers = () => invoke<CatalogView>("list_trainers");
export const reloadTrainers = () => invoke<CatalogView>("reload_trainers");
export const onTrainersChanged = (cb: () => void): Promise<UnlistenFn> => listen("trainers-changed", () => cb());
export const trainerView = () => invoke<TrainerView | null>("trainer_view");
export const cheatStates = () => invoke<CheatState[]>("cheat_states");
export const setCheat = (cheatId: string, input: CheatInput) => invoke<CheatState>("set_cheat", { cheatId, input });
/** `accelerator`: "" desactiva la drecera, `null` torna a la de la definició. */
export const setHotkey = (key: string, accelerator: string | null) => invoke<void>("set_hotkey", { key, accelerator });
export const onHotkeyFired = (cb: (e: HotkeyFired) => void): Promise<UnlistenFn> =>
  listen<HotkeyFired>("hotkey-fired", (ev) => cb(ev.payload));

// --- Biblioteca ---
export const listGames = (refresh = false) => invoke<GameEntry[]>("list_games", { refresh });
export const runningGames = () => invoke<RunningGame[]>("running_games");
export const onRunningGames = (cb: (g: RunningGame[]) => void): Promise<UnlistenFn> =>
  listen<RunningGame[]>("running-games", (ev) => cb(ev.payload));
/** Obre el selector de fitxers. `null` si l'usuari cancel·la. */
export const addManualGame = () => invoke<GameEntry[] | null>("add_manual_game");
export const removeManualGame = (path: string) => invoke<GameEntry[]>("remove_manual_game", { path });

// --- Escàner i editor ---
export const scanFirst = (valueType: ValueType, value: number) => invoke<ScanSummary>("scan_first", { valueType, value });
export const scanNext = (filter: ScanFilter) => invoke<ScanSummary>("scan_next", { filter });
export const scanResults = (offset: number, limit: number) => invoke<ScanHit[]>("scan_results", { offset, limit });
export const scanReset = () => invoke<void>("scan_reset");
export const scanWrite = (address: string, valueType: ValueType, value: number) =>
  invoke<MemoryReading>("scan_write", { address, valueType, value });
export const findPointers = (address: string) => invoke<PointerCandidate[]>("find_pointers", { address });
/** Retorna la ruta del fitxer on s'ha desat la definició. */
export const saveCheat = (cheat: CheatDef) => invoke<string>("save_cheat", { cheat });

// --- Configuració ---
export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (value: Settings) => invoke<void>("save_settings", { value });

// --- Depuració de memòria (Fase 2) ---
export const debugRead = (locator: Locator, valueType: ValueType) =>
  invoke<MemoryReading>("debug_read", { locator, valueType });
export const debugWrite = (locator: Locator, valueType: ValueType, value: number, freeze: boolean) =>
  invoke<MemoryReading>("debug_write", { locator, valueType, value, freeze });
export const listFrozen = () => invoke<string[]>("list_frozen");
export const unfreeze = (key: string) => invoke<boolean>("unfreeze", { key });

/** Error rebut d'`invoke`: `{ code, message }` si ve de Rust. */
export function asError(e: unknown): ErrorPayload {
  if (e && typeof e === "object" && "message" in e) {
    const p = e as Partial<ErrorPayload>;
    return { code: p.code ?? "UNKNOWN", message: String(p.message) };
  }
  return { code: "UNKNOWN", message: String(e) };
}

export const errorMessage = (e: unknown) => asError(e).message;
