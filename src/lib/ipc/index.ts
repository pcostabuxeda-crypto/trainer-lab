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

export type { Arch } from "./types/Arch";
export type { ErrorPayload, Locator, LogEntry, LogLevel, MemoryReading, ProcessInfo, SessionEvent, SessionInfo, ValueType };

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
