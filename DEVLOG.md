# DEVLOG

## Correcció de la cerca següent de diners (27/10/2026)

**Diagnòstic**
- Als logs de Moon Sweepers, `f32` 6,71 trobava 14.008 candidats però refinar a 6,86
  en deixava zero. Les coincidències inicials no demostraven que fossin els diners.
- L'assistent forçava `i32` o `f32` segons «Té decimals». No cercava `f64`/`i64`,
  encara que el backend ja els admetia. El format real de Moon Sweepers no s'ha confirmat.
- Les lectures fallides es descartaven sense diagnòstic.

**Canvis**
- L'assistent cerca automàticament `i32`, `i64`, `f32` i `f64`, compartint la lectura
  dels blocs. Cada candidat conserva el tipus per refinar, provar, restaurar i desar el truc.
- Solapament de 4 bytes entre blocs per no perdre valors de 8 bytes que en travessen el límit.
- Les cerques següents compten les lectures fallides i les registren. Si fallen totes,
  conserven els candidats anteriors i permeten reintentar amb un missatge específic.
- L'assistent informa de cerques truncades i de zero coincidències. Ja no demana a
  l'usuari que dedueixi el format de memòria a partir de si el número té decimals.
- Es bloqueja cancel·lar durant una cerca i repetir-la amb Enter en mode avançat mentre treballa.

**Verificat**
- 109 tests unitaris OK (1 ignorat), incloent reproducció `6,71 → 6,86` amb diners `f64`,
  tipus mixtos, límits de blocs i recuperació de lectures fallides.
- 4 tests d'integració OK (benchmark ignorat), incloent diners `i64` del joc de prova
  cercats automàticament i refinats després de comprar.
- `npm run build` OK; `cargo clippy -p trainer-lab --lib --tests` sense avisos de codi.
- Els logs nous de Moon Sweepers mostren una cerca automàtica de 8,79 amb 2.047 candidats;
  «Ha pujat» en conserva 1, sense errors de lectura, i la prova escriu 999 sense error.
  Això confirma que la cerca següent ja avança en aquesta sessió; falta la confirmació
  visual de l'usuari que el candidat correspon als diners.
- `cargo test --workspace` no ha pogut substituir l'executable perquè l'app estava oberta;
  els tests d'integració s'han executat directament des del binari de tests acabat de compilar.
  Els tests unitaris han necessitat una carpeta temporal dins del workspace pel sandbox.
- La comprovació estricta de Clippy de tot el workspace detecta un avís preexistent de
  documentació (`doc_lazy_continuation`) a `test-game/src/main.rs`.

**Pendent de validar amb el joc de l'usuari**
- Confirmar que el candidat de Moon Sweepers modifica els diners al joc. La cerca automàtica no converteix
  escales (cèntims, K/M) ni segueix dades que el joc trasllada entre cerques.

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

## Fase 5 — Escàner de memòria i editor de trainers (27/10/2026)

**Fet**
- `memory/scanner.rs`: primera cerca per valor exacte en totes les regions escrivibles
  (alineada, en blocs de 4 MB, màxim 2 M de resultats) i cerques següents (exacte, ha
  canviat, no ha canviat, ha augmentat, ha disminuït) rellegint per blocs de 64 KB.
- Cerca de punters estàtics d'un nivell (`find_static_pointers`) per convertir una adreça del
  heap en un localitzador que sobreviu als reinicis.
- `trainers/editor.rs`: «Desa com a truc» afegeix el truc a la definició de l'usuari, en copia
  l'exemple inclòs o en crea una de nova. Validació completa i escriptura atòmica.
- Sessió: estat de l'escaneig per joc adjuntat i `reload_trainer` després de desar.
- UI: pantalla **Escàner** (tipus, valor, condicions, resultats en directe quan n'hi ha
  ≤ 100, Escriu) i diàleg **Desa com a truc** (nom, categoria, tipus de truc, valors,
  drecera, localitzador editable i cerca de punter estàtic).
- S'ha esborrat el component `Placeholder`, que ja no feia servir cap pantalla.

**Verificat**
- `cargo test --workspace`: 102 tests unitaris + 3 d'integració. El nou fa el recorregut
  amb el joc real: primera cerca de 100, rep mal, «exacte 85» + «no ha canviat», la vida és
  entre els resultats, és dinàmica, i la cerca de punters troba l'àncora de `test-game.exe`
  amb offset 0.
- `cargo clippy` sense avisos; `npm run typecheck` i `vite build` OK.
- Flux de la UI provat al navegador amb el backend simulat: cerca, refinar, desar com a truc,
  cercar el punter i enviar un `CheatDef` vàlid.

**No verificat**
- «Desa com a truc» de punta a punta a l'app real (la recàrrega de la sessió no té test
  automàtic; l'escriptura i la validació sí).

## Mode fàcil (27/10/2026)

Objectiu: que fer servir i crear trucs sigui tan senzill com en les apps comercials, amb
un disseny propi.

**Fet**
- **Juga** a la Biblioteca: obre el joc per la seva botiga (`steam://rungameid/…`,
  `com.epicgames.launcher://…`) o executant-lo (GOG i manuals). Només jocs que l'app ha
  trobat, mai una ruta enviada per la UI.
- **Connexió automàtica**: 5 s després de veure obert un joc amb trucs, l'app s'hi connecta.
  Un sol intent per procés: si et desconnectes, no s'hi torna a connectar. Si cal confirmar un
  avís d'anti-cheat, no ho fa sola: ho deixa a la capçalera («Connecta»).
- **Mode avançat** (Configuració, desactivat per defecte): adreces, PID, eines de depuració,
  escàner tècnic i llista de processos. Sense, la UI només mostra jocs i trucs.
- **Assistent «Crea trucs»**: què → valor → canvia'l → prova cada candidat (i restaura els que
  no són) → com el vols. Busca sol el punter estàtic, tria la primera tecla F lliure i desa
  la definició. Si no hi ha camí estable, aplica un **truc temporal** (`freeze_address`) que
  dura fins que tanques el joc.
- Textos més planers: «Connecta», «Desconnecta», «té trucs», «Trucs», «Crea trucs».

**Verificat**
- `cargo test --workspace`: 104 tests unitaris + 3 d'integració; clippy sense avisos.
- Connexió automàtica a l'app real: ha detectat el joc de prova i, com que Vanguard és en
  marxa, s'ha esperat a la confirmació (comportament previst).
- Assistent de punta a punta al navegador amb el backend simulat.

**No verificat**
- «Juga» obrint de debò un joc de Steam o Epic.

### Recordar l'avís d'anti-cheat per joc (27/10/2026)

- `settings.json` guarda `acknowledgedWarnings`: ruta de l'exe → avisos acceptats. Si el joc
  torna a donar els mateixos avisos, l'app s'hi connecta sense preguntar (també la connexió
  automàtica). Si n'apareix un de nou, torna a preguntar. Els anti-cheats **dins** del joc
  continuen bloquejant sempre.
- Configuració → «Avisos d'anti-cheat acceptats»: llista per joc, «Oblida» i «Oblida-ho tot».
- **Error corregit:** a Chromium recent `scrollIntoView` retorna una Promise. `LogViewer` la
  retornava des d'un `useEffect`, React la prenia per una funció de neteja i la pantalla de
  Configuració es quedava en blanc. Ara els efectes fan servir claus.
- Verificat: 105 tests unitaris, clippy, typecheck. Configuració provada al navegador amb dades
  simulades (llistar, oblidar un, oblidar-ho tot).

### L'app es quedava «No respon» durant les cerques (27/10/2026)

**Causa:** la primera cerca a Moon Sweepers trigava ~40 s i tenia la sessió bloquejada tota
l'estona. Algunes comandes síncrones (que Tauri executa al fil de la finestra) esperaven aquest
bloqueig, i Windows marcava la finestra com a «No respon».

**Solució**
- L'escàner treballa amb el procés fora del bloqueig de la sessió i desa els resultats al final
  (només si la sessió encara és del mateix joc).
- Totes les comandes que toquen la sessió són `async`: mai s'executen al fil de la finestra.
- Primera cerca més ràpida: comparació directa de bytes (enters) o de `f32`/`f64` (decimals),
  i el crate de l'app es compila amb `opt-level = 3` també en desenvolupament.
- Es salta la memòria mapejada (`MEM_MAPPED`), com fa Cheat Engine per defecte.

**Mesura** (test ignorat `scan_benchmark`, Moon Sweepers, només lectura): `f32` 6,71 de
~40 s → **4,3 s**; `i32` 100 → 5,0 s. 105 tests unitaris + 3 d'integració, clippy net.

### Moon Sweepers: el truc creat no canviava els diners (27/10/2026)

**Diagnòstic** (captura del joc + cerques de només lectura)
- El joc mostra «47.70 $». La cerca automàtica de Codex arribava a 1 candidat, però escriure-hi
  999 no canviava la pantalla; en respondre «No», l'assistent restaurava el valor i el truc es perdia.
- Cerca a 47,70: 41 `f32`, 35 `f64`, 542 `i32` = 4770, 8 `i64` = 4770. Els cèntims són una
  possibilitat real que l'assistent no buscava mai.

**Canvis**
- **Relectura al pas «Prova»**: després d'escriure, s'espera 1,2 s i es torna a llegir. Si el joc
  ho ha canviat, és una còpia que el joc recalcula i es passa sola al següent candidat. Si s'ha
  mantingut, s'avisa que el joc potser no refresca el número fins que canvia per una altra raó.
- **Cèntims**: si el número té decimals, la cerca automàtica també busca `i32`/`i64` × 100. Cada
  candidat porta `scale`; les cerques següents comparen en unitats de la pantalla i l'assistent
  escriu `valor × scale`. A la mateixa adreça es prova primer el tipus més petit.

**Verificat**: 110 tests unitaris + 4 d'integració, clippy net, typecheck i build.
**Pendent**: provar-ho a Moon Sweepers amb l'usuari (cal canviar els diners jugant).

### Prova a Moon Sweepers feta per Claude (27/10/2026)

**Mètode**: assistent i escàner avançat de l'app + captures del joc. El menú Construir fa
d'oracle («Te faltan $X» = 250 − diners reals). Cada valor de prova s'ha restaurat.

**Resultats**
- Diners reals ≈ 47,707 (el HUD els retalla a 47.70; el menú diu que falten 202,29).
- 8 `f64` en clar ≈ 47,7074 → escriure-hi 999 (tots alhora) **no** canvia el menú.
- Cap `f32` ≈ 47,7074. Enters: cap amb ×100 (que seria 4770; el menú ho descarta), ×10.000 ni
  ×100.000; un `i64` = 47707 (×1000) en adreça no alineada: tampoc canvia el menú.
- `global-metadata.dat` (IL2CPP): `shownMoney`, `shadowMoney`, `MoneyChanged`,
  `LocalBuildMoney`… i el joc té «Tu tripulación» (cooperatiu). Els diners no són en clar:
  els protegeix el mateix joc (còpia ombra / sincronització).

**Decisió**: no s'intenta saltar aquesta protecció (requisit del projecte: cap bypass
d'anti-cheat). L'assistent funciona; és aquest joc el que no ho permet per als diners.
Els tests ignorats `scan_dump` i `scan_list` queden com a eines de diagnòstic.
