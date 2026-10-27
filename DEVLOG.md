# DEVLOG

## Fase 0A — Anàlisi de referència (27/10/2026)

- WeMod s'ha rebatejat com a **Wand** i s'instal·la a `%LOCALAPPDATA%\Wand`.
- Analitzat amb REA i lectura del JS: Electron 41, Aurelia + React, servei .NET per a la
  injecció, trainers en forma de DLL natives per joc. Detalls a
  [docs/ANALISI_REFERENCIA.md](docs/ANALISI_REFERENCIA.md).
- Les còpies i l'ASAR extret són a `research/` (ignorada per git).

## Fase 0B — Pla d'arquitectura (27/10/2026)

- Decisions: Tauri 2, accés extern a la memòria (sense injecció), definicions JSON
  validades a Rust amb JSON Schema generat, tipus TS amb `ts-rs` i persistència en JSON.
- Pla complet a [docs/PLA_ARQUITECTURA.md](docs/PLA_ARQUITECTURA.md).

## Fase 1 — Esquelet, joc de prova i processos (27/10/2026)

**Fet**
- Workspace Cargo amb dos crates: `src-tauri` (app) i `test-game`.
- Frontend: React 18 + TypeScript + Vite 8 + Tailwind 4 + Zustand. Navegació amb 4
  pantalles (Biblioteca, Trainer, Escàner, Configuració) i tema fosc propi.
- Backend:
  - `memory/process.rs`: llista processos amb Toolhelp32. Per a cada procés obté la ruta
    (`QueryFullProcessImageNameW`) i l'arquitectura (`IsWow64Process2`), amb el permís mínim
    `PROCESS_QUERY_LIMITED_INFORMATION`. Els handles es tanquen sols (`OwnedHandle` + `Drop`).
  - `logging.rs`: `tracing` cap a un fitxer diari, la consola (en desenvolupament) i un
    buffer circular de 2.000 línies. La UI el consulta (`get_logs`) i el rep en directe
    (esdeveniment `log`).
  - `error.rs`: `AppError` serialitzat com `{ code, message }`.
  - Tipus TS generats amb `ts-rs` a `src/lib/ipc/types/`.
- Joc de prova: `Player` al heap i `static ANCHOR` amb el magic `TLABANCH` + punter.
  Comprovat que el patró apareix un sol cop a l'executable.
- Capability de Tauri reduïda a `core:default` i CSP restrictiva.

**Verificat**
- `cargo test`: 8 tests OK. Inclouen el mapatge d'arquitectures, el procés actual detectat
  com a x64 amb ruta, el buffer de logs i l'exportació de tipus.
- `npm run typecheck` i `vite build` OK.
- `npm run tauri dev`: l'app arrenca, crida `list_processes` (376 processos) i escriu
  els logs.
- Layout revisat al navegador amb dades simulades.

**Notes**
- La plantilla portava React 19. S'ha fixat a **React 18**, tal com demana l'especificació.
- En desenvolupament, `list_processes` es crida dos cops a l'arrencada per `StrictMode`
  de React. És normal i no passa en producció.
