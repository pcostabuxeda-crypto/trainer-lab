# Trainer Lab

Aplicació d'escriptori per a Windows per crear i fer servir **trainers** (trucs) en jocs
**single-player o offline**. Els trucs es defineixen en fitxers JSON. L'app llegeix i
escriu la memòria del joc **des de fora**, sense injectar codi.

> **Estat:** Fase 3 de 5. Els trucs es defineixen en JSON, tenen pantalla pròpia i dreceres
> globals. Vegeu [DEVLOG.md](DEVLOG.md), [com funciona](docs/ARCHITECTURE.md) i el
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

A la pantalla **Biblioteca** de l'app hauria de sortir `test-game.exe` marcat com a
«joc de prova».

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

1. **Troba el valor.** Amb el joc obert, localitza l'adreça de la vida, els diners, etc. De
   moment ho pots fer amb el panell de depuració de la pantalla Trainer o amb una eina externa.
   L'escàner integrat arriba a la Fase 5.
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

## Limitacions conegudes

- Encara no detecta els jocs instal·lats (Fase 4) ni té escàner de memòria (Fase 5).
- Algunes dreceres poden no arribar a jocs que capturen el teclat a baix nivell (p. ex. en
  pantalla completa exclusiva). Si passa, prova una combinació amb Ctrl o Alt.
- Només es pot adjuntar a un joc alhora.
- Els processos protegits del sistema surten sense ruta ni arquitectura: és normal,
  Windows no deixa consultar-los sense privilegis.

## Llicència

Privada. Vegeu [LICENSE](LICENSE).
