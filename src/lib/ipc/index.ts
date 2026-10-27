// Wrapper tipat de les comandes i esdeveniments de Rust. Els tipus de ./types els genera
// ts-rs (`npm run types`): no s'editen a mà.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ErrorPayload } from "./types/ErrorPayload";
import type { LogEntry } from "./types/LogEntry";
import type { LogLevel } from "./types/LogLevel";
import type { ProcessInfo } from "./types/ProcessInfo";

export type { Arch } from "./types/Arch";
export type { ErrorPayload, LogEntry, LogLevel, ProcessInfo };

export const listProcesses = () => invoke<ProcessInfo[]>("list_processes");

export const getLogs = (limit?: number, minLevel?: LogLevel) =>
  invoke<LogEntry[]>("get_logs", { limit, minLevel });

export const onLog = (cb: (e: LogEntry) => void): Promise<UnlistenFn> =>
  listen<LogEntry>("log", (ev) => cb(ev.payload));

/** Text llegible d'un error rebut d'`invoke` (AppError de Rust o qualsevol altra cosa). */
export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as ErrorPayload).message);
  return String(e);
}
