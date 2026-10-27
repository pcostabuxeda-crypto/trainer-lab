# Trainer Lab

Aplicació d'escriptori per a Windows per crear i fer servir **trainers** (trucs) en jocs
**single-player o offline**. Els trucs es defineixen en fitxers JSON. L'app llegeix i
escriu la memòria del joc **des de fora**, sense injectar codi.

> **Estat:** les 5 fases fetes. Biblioteca (Steam, Epic, GOG), trucs definits en JSON amb
> dreceres globals, i escàner de memòria per crear-ne de nous. Vegeu [DEVLOG.md](DEVLOG.md), [com funciona](docs/ARCHITECTURE.md) i el
> [pla d'arquitectura](docs/PLA_ARQUITECTURA.md).

## Límits que l'app respecta

- Només jocs **single-player o offline**. Si detecta un anti-cheat (EasyAntiCheat,
  BattlEye, Vanguard…) o un joc marcat com a online, **no s'hi connecta**.
- No hi ha cap bypass d'anti-cheat, injecció de codi ni tècnica d'ocultació.
- Funciona sense administrador. Només ho demanarà, explicant per què, si el joc
  s'executa com a administrador.

## Requisits

- Windows 10/11 x64 amb WebView2 (ja ve amb Windows 11)
- [Rust](https://rustup.rs) estable (toolchain MSVC) i Visual Studio Build Tools amb C++
- [Node.js](https://nodejs.org) 20 o superior

## Instal·lació i execució en desenvolupament

```bash
npm install
```

```bash
npm run tauri dev
```

En un altre terminal, obre el joc de prova:

```bash
npm run test-game -- --debug
```

A la capçalera de l'app sortirà «Detectat Trainer Lab · joc de prova» amb un botó
**Adjunta**.

## La Biblioteca

- Busca els jocs instal·lats a **Steam** (totes les biblioteques de `libraryfolders.vdf`),
  **Epic Games** (manifests de `%ProgramData%`) i **GOG Galaxy** (registre).
- **Afegeix .exe** per als jocs que no surten (altres botigues o sense botiga).
- Cada joc mostra si té trainer. La definició es reconeix pel camp `match` (id de Steam, Epic
  o GOG) o, si no n'hi ha, buscant l'executable de la definició dins la carpeta del joc.
- Cada 2 segons mira quins jocs estan en marxa. Si n'hi ha un amb trainer, el proposa a la
  capçalera.

## Ordres útils

| Ordre | Què fa |
|---|---|
| `npm run tauri dev` | App en mode desenvolupament (recàrrega en calent del frontend) |
| `npm run tauri build` | Instal·lador NSIS a `target/release/bundle/` |
| `npm run test-game -- --debug` | Joc de prova. `--debug` mostra les adreces de memòria |
| `cargo test --workspace` | Tests de Rust (també regenera els tipus TS) |
| `npm run types` | Regenera els tipus TypeScript i el JSON Schema de les definicions |
| `npm run typecheck` | Comprova els tipus del frontend |

## Estructura

```
src/                 Frontend React + TypeScript + Tailwind + Zustand
  app/               Layout i navegació
  components/ui/     Components reutilitzables
  features/          library · trainer · settings (scanner a la Fase 5)
  lib/ipc/           Crides tipades a Rust (types/ el genera ts-rs)
  stores/            Estat global (Zustand)
src-tauri/           Backend Rust (Tauri 2)
  src/memory/        Accés a processos
  src/logging.rs     Logs: fitxer + consulta des de la UI
  src/commands.rs    Comandes que pot cridar el frontend
test-game/           Joc de prova (vida, munició, diners, velocitat)
trainers/            Definicions d'exemple (test-game.json)
schemas/             JSON Schema de les definicions (generat amb `npm run types`)
docs/                Anàlisi de referència, pla d'arquitectura i com funciona
```

Els logs es desen a `%LOCALAPPDATA%\cat.trainerlab.app\logs\` i també es poden veure a
**Configuració → Logs**.

## Com crear un trainer nou

Exemple complet i comentat: [trainers/test-game.json](trainers/test-game.json). Format
detallat: [docs/PLA_ARQUITECTURA.md §4](docs/PLA_ARQUITECTURA.md#4-format-de-definició-de-trainers).

1. **Troba el valor amb l'escàner** (vegeu la secció següent). Des del mateix escàner pots
   **Desa com a truc**: l'app crea o amplia la definició i et porta a la pantalla del trainer.
   Els passos d'aquí sota són per escriure o retocar la definició a mà.
2. **Fes-la estable.** Una adreça absoluta canvia a cada partida. Fes servir:
   - `"offset"` + `"pointers"`: mòdul + offset fix i la cadena de punters fins al valor;
   - o `"aob"`: un patró de bytes que no canviï entre versions, més `"pointers"`.
3. **Crea el fitxer** `<id-del-joc>.json` a la carpeta d'usuari. La ruta surt a
   **Biblioteca → Trainers**; normalment és `%APPDATA%\cat.trainerlab.app\trainers`.
   Afegeix `"$schema"` apuntant a [schemas/trainer.schema.json](schemas/trainer.schema.json)
   i VS Code t'autocompletarà els camps.
4. **Tria el tipus de cada truc:**

   | `kind` | Què fa | Camps clau |
   |---|---|---|
   | `toggle` | Activar/desactivar | `on: { freeze }`, `on: { write }` o `on: { patch, expect }`; `off`; `hotkey` |
   | `slider` | Valor dins d'un rang | `min`, `max`, `step`, `freeze`, `hotkeys: { increase, decrease }` |
   | `value` | Camp + «Aplica» | `default`, `min`, `max` |
   | `button` | Acció puntual | `action: { add }` o `action: { set }`; `hotkey` |

5. **Desa.** L'app recarrega el fitxer sola. Si hi ha errors, surten a **Biblioteca →
   Trainers** amb la ruta del camp (p. ex. `cheats[2].on.expect: …`).
6. **Adjunta't al joc.** La pantalla Trainer mostra els trucs, el valor actual i l'estat de la
   versió.
7. **(Opcional) Fixa la versió.** Copia l'SHA-256 que surt en passar el ratolí per sobre de
   l'etiqueta de versió i afegeix-lo a `versions`:
   `{ "label": "1.0.2", "sha256": "…" }`.

Les dreceres per defecte són les del JSON. Per canviar-les, clica-hi a la pantalla Trainer i
prem la combinació nova. **Supr** la desactiva i **↺** torna a la del fitxer.

## L'escàner de memòria

1. Adjunta't al joc i obre **Escàner**.
2. Tria el tipus (`i32` serveix per a la majoria de comptadors; `f32` per a barres i
   velocitats) i escriu el valor que veus al joc. **Primera cerca**.
3. Canvia el valor jugant (rep mal, gasta diners) i fes **Cerca següent** amb la condició que
   toqui: *valor exacte*, *ha disminuït*, *ha augmentat*, *ha canviat* o *no ha canviat*.
4. Quan en quedin pocs, els valors s'actualitzen sols. Prova **Escriu** per confirmar quina
   és la bona.
5. **Desa com a truc**:
   - si l'adreça és **estàtica** (`joc.exe+0x…`), ja es pot desar tal qual;
   - si és **dinàmica** (al heap), prem **Busca un punter estàtic**. Si l'app troba una
     variable global que hi apunta, el truc funcionarà després de reiniciar el joc.

## Limitacions conegudes

- L'escàner no té «valor inicial desconegut»: la primera cerca sempre és amb un valor exacte.
- La cerca de punters és d'**un nivell** (variable global → objecte → camp). Les cadenes més
  llargues s'han de trobar amb una altra eina i escriure a mà al localitzador.
- Desar un truc nou reinicia els trucs actius del joc (es restauren els valors originals).
- Altres botigues (EA app, Ubisoft Connect, Xbox, Battle.net…) no es detecten soles:
  afegeix-les amb **Afegeix .exe**.
- Algunes dreceres poden no arribar a jocs que capturen el teclat a baix nivell (p. ex. en
  pantalla completa exclusiva). Si passa, prova una combinació amb Ctrl o Alt.
- Només es pot adjuntar a un joc alhora.
- Els processos protegits del sistema surten sense ruta ni arquitectura: és normal,
  Windows no deixa consultar-los sense privilegis.

## Llicència

Privada. Vegeu [LICENSE](LICENSE).
