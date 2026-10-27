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

## Fase 2 — Memòria, pointer chains i congelació (27/10/2026)

**Fet**
- `memory/mod.rs`: trait `Memory` (llegir, escriure, mida de punter, regions) i
  `check_write`, que valida cada escriptura (regió amb commit, no guard, dins de límits,
  escrivible o executable).
- `memory/process.rs`: `Process::open` amb permisos mínims (`VM_READ/WRITE/OPERATION`,
  `QUERY_LIMITED_INFORMATION`, `SYNCHRONIZE`). Mòduls amb Toolhelp (inclosos els de 32 bits),
  `VirtualQueryEx`, patches de codi amb `VirtualProtectEx` + `FlushInstructionCache`.
- `memory/value.rs`: 11 tipus (`i8`…`f64`, `bool`) amb control de rang.
- `memory/pointer.rs`: pointer chains amb punters de 4 o 8 bytes i error `NullPointer` per pas.
- `memory/aob.rs`: parser de patrons amb comodins i escaneig per regions en blocs solapats.
- `memory/locator.rs`: el format `address` de les definicions (mòdul+offset o AOB, `rip`,
  `pointers`). Offsets en hex o decimal.
- `memory/freeze.rs`: fil que reescriu els valors congelats cada 100 ms. Deixa de congelar
  un valor després de 20 errors seguits.
- `anticheat.rs`: bloqueja si l'anti-cheat és dins del joc i avisa (amb confirmació) si
  només és en marxa al sistema. Decidit amb l'usuari perquè Vanguard s'executa sempre.
- `session.rs`: adjuntar/desadjuntar, fil vigilant que detecta el tancament del joc i
  esdeveniment `session-state` cap a la UI.
- UI: botó «Adjunta» a la llista de processos, diàleg de confirmació d'anti-cheat, pantalla
  Trainer amb un panell de depuració (presets del joc de prova, llegir, escriure, congelar).
- `docs/ARCHITECTURE.md`: memòria virtual, RPM/WPM, pointer chains, AOB, RIP, congelació.

**Verificat**
- `cargo test --workspace`: 37 tests unitaris + 1 d'integració. El d'integració obre el
  `test-game` real, troba la vida per AOB i punter, l'escriu, la congela, comprova que el joc
  veu els canvis i detecta el tancament.
- `npm run typecheck` OK. L'app de `tauri dev` s'ha recompilat amb les comandes noves.
- Flux de la UI provat al navegador amb el backend simulat: avís d'anti-cheat → confirmació →
  pantalla Trainer → congelar.

**Notes**
- Els valors viatgen com a `f64`: els enters de més de 2^53 perden precisió (comentat al codi).
- `Locator.pointers` és opcional al JSON (per defecte, cap punter).
- El crate només compila a Windows (`compile_error!` explícit).

## Fase 3 — Definicions, UI del trainer i dreceres (27/10/2026)

**Fet**
- `trainers/model.rs`: format de definicions amb `serde` (`deny_unknown_fields`), validació de
  coherència amb la ruta de cada error i JSON Schema generat (`schemas/trainer.schema.json`).
- `trainers/mod.rs`: catàleg de definicions (exemples + carpeta d'usuari, que té prioritat),
  errors per fitxer, hot-reload amb `notify` i SHA-256 de l'executable amb memòria cau.
- `trainers/engine.rs`: toggle (freeze/write/patch amb `expect`), slider (rang, step,
  congelació), value i button (add/set). Desa els originals i els restaura en desactivar o
  desadjuntar.
- La congelació ara recorre la cadena de punters a cada cicle (`Freezer::set_chain`): segueix
  l'objecte si el joc el mou.
- `hotkeys.rs`: dreceres globals registrades només amb un joc adjuntat; dreceres
  personalitzades a `settings.json`; esdeveniment `hotkey-fired`.
- `settings.rs`: interval de congelació i dreceres. Escriptura atòmica i còpia `.bak` si el
  fitxer és invàlid.
- Sessió: la definició `online: true` bloqueja; l'arquitectura ha de coincidir; avís (amb
  confirmació) si hi ha fitxers d'anti-cheat a la carpeta del joc.
- UI: llista de trainers i errors a la Biblioteca, processos marcats «té trainer», pantalla del
  trainer amb trucs per categoria, valors en directe (cada 500 ms), editor de dreceres,
  etiqueta de versió i configuració amb l'interval de congelació.
- `trainers/test-game.json`: exemple amb un truc de cada tipus.

**Verificat**
- `cargo test --workspace`: 78 tests unitaris + 2 d'integració amb el joc real. El nou aplica
  `test-game.json` sencer: vida congelada, +50 de munició, diners, velocitat i restauració.
- `cargo clippy` sense avisos. `npm run typecheck` i `vite build` OK.
- Hot-reload comprovat amb l'app en marxa: desar el JSON en torna a carregar el catàleg.
- Pantalla del trainer revisada al navegador amb el backend simulat.

**No verificat**
- Les dreceres globals amb un joc real a primer pla: l'eina de control de pantalla no
  reconeix l'exe de `tauri dev`. El registre i el handler compilen i tenen test de la lògica
  de prioritats.

## Fase 4 — Biblioteca: Steam, Epic, GOG i jocs en execució (27/10/2026)

**Fet**
- `games/vdf.rs`: parser propi del format KeyValues de Valve (cometes, escapes, blocs, comentaris).
- `games/registry.rs`: lectura del registre (`RegGetValueW`, `RegEnumKeyExW`), només lectura.
- `games/steam.rs`, `epic.rs`, `gog.rs`: proveïdors per botiga. Steam llegeix totes les
  biblioteques i descarta les instal·lacions incompletes i els paquets que no són jocs; Epic
  descarta complements i instal·lacions a mig fer.
- Jocs manuals (`Afegeix .exe`, amb el selector de fitxers de `tauri-plugin-dialog`), desats a
  `settings.json`.
- Coincidència joc ↔ definició: camp nou `match` (`steamAppId`, `epicAppName`, `gogId`), exe
  principal o cerca de l'exe a la carpeta del joc. Es refà quan canvien les definicions.
- Fil que detecta els jocs en marxa cada 2 s i emet `running-games`.
- UI: graella de jocs (botiga, trainer, en marxa, Adjunta), jocs en marxa sense botiga, filtre
  «només amb trainer», cerca, i «Detectat … Adjunta» a la capçalera. La llista de tots els
  processos queda plegada per als casos especials.

**Verificat**
- `cargo test --workspace`: 91 tests unitaris + 2 d'integració; clippy sense avisos.
- Escaneig real d'aquest PC (test ignorat `scans_this_pc`): 15 jocs de Steam + 3 d'Epic en ~5 ms.
  A l'app, amb la cerca d'executables, 18 jocs en 345 ms i el joc de prova detectat en marxa.
- Biblioteca revisada al navegador amb dades simulades.

**Notes**
- GOG no està instal·lat en aquest PC: el proveïdor no s'ha provat amb dades reals.
