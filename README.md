# Trainer Lab

Aplicació d'escriptori per a Windows per crear i fer servir **trainers** (trucs) en jocs
**single-player o offline**. Els trucs es defineixen en fitxers JSON. L'app llegeix i
escriu la memòria del joc **des de fora**, sense injectar codi.

> **Estat:** Fase 2 de 5. Ja es pot adjuntar a un joc i llegir, escriure i congelar valors des del
> panell de depuració. Vegeu [DEVLOG.md](DEVLOG.md), [com funciona](docs/ARCHITECTURE.md) i el
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
| `npm run types` | Regenera només els tipus TypeScript a `src/lib/ipc/types/` |
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
docs/                Anàlisi de referència i pla d'arquitectura
```

Els logs es desen a `%LOCALAPPDATA%\cat.trainerlab.app\logs\` i també es poden veure a
**Configuració → Logs**.

## Com crear un trainer nou

Arriba a la **Fase 3**, quan el format de definició estigui implementat. El format previst
és a [docs/PLA_ARQUITECTURA.md §4](docs/PLA_ARQUITECTURA.md#4-format-de-definició-de-trainers).

## Limitacions conegudes

- Encara no hi ha trucs definits per JSON ni hotkeys (Fase 3): de moment es fa servir el panell de depuració.
- Només es pot adjuntar a un joc alhora.
- Els processos protegits del sistema surten sense ruta ni arquitectura: és normal,
  Windows no deixa consultar-los sense privilegis.

## Llicència

Privada. Vegeu [LICENSE](LICENSE).
