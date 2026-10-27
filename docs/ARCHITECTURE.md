# Arquitectura i conceptes de baix nivell

Aquest document explica **com** Trainer Lab llegeix i modifica la memòria d'un joc. Està
pensat per aprendre: cada secció enllaça amb el codi que la implementa. El pla general
(mòduls, comandes, format de definicions) és a [PLA_ARQUITECTURA.md](PLA_ARQUITECTURA.md).

---

## 1. Memòria virtual en 2 minuts

Cada procés de Windows té el seu propi **espai d'adreces virtual**. L'adreça `0x7FF6…` del
joc no és la mateixa que la `0x7FF6…` de Trainer Lab: cada procés veu el seu món.

Aquest espai està dividit en **regions** (blocs de pàgines de 4 KB) amb:

- **Estat:** `MEM_COMMIT` (té memòria de debò), `MEM_RESERVE` (reservada, buida) o `MEM_FREE`.
- **Protecció:** `PAGE_READWRITE` (dades), `PAGE_EXECUTE_READ` (codi), `PAGE_READONLY`
  (constants), `PAGE_GUARD` (pàgina trampa que llança una excepció si la toques)…

Un executable (`.exe`) o una DLL carregats formen un **mòdul**: comencen en una **adreça
base** i ocupen diverses regions (`.text` = codi, `.data` = variables globals, `.rdata` =
constants…). Per seguretat (ASLR), la base canvia a cada execució, però **dins del mòdul
els offsets són fixos**. Per això es parla de `game.exe+0x31000` i no d'una adreça absoluta.

Al codi: [`Region`](../src-tauri/src/memory/mod.rs) i [`Process::region`](../src-tauri/src/memory/process.rs)
(fa servir `VirtualQueryEx`).

---

## 2. Llegir i escriure memòria d'un altre procés

Windows ofereix tres crides, i no cal injectar res al joc:

```text
OpenProcess(permisos, pid)            → un HANDLE al procés
ReadProcessMemory(handle, adreça, buffer, mida)
WriteProcessMemory(handle, adreça, dades, mida)
```

Trainer Lab demana **només** aquests permisos ([`Process::open`](../src-tauri/src/memory/process.rs)):

| Permís | Per a què |
|---|---|
| `PROCESS_VM_READ` | `ReadProcessMemory` |
| `PROCESS_VM_WRITE` + `PROCESS_VM_OPERATION` | `WriteProcessMemory` i `VirtualProtectEx` |
| `PROCESS_QUERY_LIMITED_INFORMATION` | ruta de l'exe i arquitectura |
| `SYNCHRONIZE` | `WaitForSingleObject`: saber quan es tanca el joc |

Si el joc i Trainer Lab són del mateix usuari, **no cal administrador**. Si el joc
s'executa elevat, `OpenProcess` retorna `ERROR_ACCESS_DENIED` i la UI explica per què.

### Validar abans d'escriure

Escriure on no toca pot fer petar el joc. Abans de cada escriptura,
[`check_write`](../src-tauri/src/memory/mod.rs) comprova amb `VirtualQueryEx` que:

1. l'adreça no és nul·la (< `0x10000`);
2. la regió té `MEM_COMMIT` i no és `PAGE_GUARD`;
3. l'escriptura no sobresurt del final de la regió;
4. la regió és escrivible (dades) o executable (patch de codi).

### Patches de codi

El codi és a pàgines `PAGE_EXECUTE_READ`, de només lectura. Per canviar-ne bytes (p. ex.
substituir `dec [rbx+10]` per `nop`):

```text
VirtualProtectEx(adreça, mida, PAGE_EXECUTE_READWRITE, &antiga)   // desprotegir
WriteProcessMemory(...)                                         // escriure
VirtualProtectEx(adreça, mida, antiga, ...)                       // restaurar
FlushInstructionCache(...)                                      // que la CPU no executi bytes vells
```

### 32 i 64 bits

Trainer Lab sempre és x64, però pot obrir jocs x86 (WoW64). Canvien dues coses:
els **punters fan 4 bytes** en lloc de 8 ([`Memory::read_pointer`](../src-tauri/src/memory/mod.rs)), i
per veure els mòduls cal `TH32CS_SNAPMODULE32`.

---

## 3. Pointer chains

Les dades interessants (vida, diners) viuen al **heap**: memòria que el joc demana en
temps d'execució i que canvia d'adreça a cada partida. Però sempre hi ha un camí fix:

```text
game.exe+0x31008  ──►  [punter]  ──►  objecte Player  (+0x00 vida, +0x08 diners…)
   (estàtic)             8 bytes         al heap
```

Una **pointer chain** descriu aquest camí. Notació de Cheat Engine: `[[base] + 0x18] + 0x40`.

```text
addr = base
per a cada offset:
    addr = llegeix_punter(addr) + offset
→ addr és l'adreça del valor
```

Al codi: [`pointer::resolve`](../src-tauri/src/memory/pointer.rs). Si un punter és 0 (el joc
encara no ha creat l'objecte, p. ex. al menú principal), retorna `NullPointer` i diu a
quin pas ha fallat.

Al JSON de les definicions:

```json
{ "module": "game.exe", "offset": "0x31008", "pointers": ["0x0"] }
```

---

## 4. AOB scanning

Quan el joc s'actualitza, `0x31008` pot passar a ser `0x31A40` i el truc deixa de funcionar.
Un **AOB** (*Array Of Bytes*) busca un patró de bytes en lloc d'una adreça:

```text
48 8B 05 ?? ?? ?? ?? 48 85 C0
```

Els `??` són comodins per als bytes que canvien entre versions (adreces, offsets). Els
fixos són les instruccions, que solen sobreviure a les actualitzacions.

Com funciona [`aob::scan`](../src-tauri/src/memory/aob.rs):

1. Recorre les regions del mòdul amb `VirtualQueryEx` i salta les no llegibles.
2. Llegeix cada regió a blocs d'1 MB. Els blocs **es solapen** `mida_patró − 1` bytes perquè
   no se n'escapi un de partit entre dos blocs (hi ha un test per a això).
3. Compara byte a byte i ignora els `??`.

Regles de seguretat del parser: mínim 4 bytes fixos i no pot començar amb `??`.

### Instruccions relatives a RIP (x64)

En x64, el codi accedeix a variables globals així: `mov rax, [rip + 0x12345]`. Els 4 bytes
`??` del patró d'abans són aquest desplaçament (*displacement*). Per obtenir l'adreça de
la variable:

```text
adreça = inici_instrucció + longitud_instrucció + disp32
```

Al JSON: `"rip": { "dispOffset": 3, "instrLength": 7 }`. Al codi: [`locator::resolve`](../src-tauri/src/memory/locator.rs).

### El joc de prova

`test-game` té una variable estàtica `ANCHOR` que comença amb els bytes `TLABANCH`
(`54 4C 41 42 41 4E 43 48`) seguits d'un punter al `Player`. Per trobar la vida:

```json
{ "aob": { "pattern": "54 4C 41 42 41 4E 43 48", "offset": 8 }, "pointers": ["0x0"] }
```

AOB → `ANCHOR.player` (magic + 8) → llegir el punter → `+0x0` = vida. Funciona encara que
recompilis el joc i canviïn totes les adreces.

---

## 5. Congelar valors

Un valor no es pot "bloquejar" de debò: el joc hi continua escrivint. **Congelar** vol dir
reescriure'l contínuament ([`Freezer`](../src-tauri/src/memory/freeze.rs)):

- Un fil per sessió, cada 100 ms per defecte (configurable).
- Si una escriptura falla 20 cops seguits (l'objecte s'ha destruït), es deixa de congelar
  aquell valor i es registra un avís.
- Un interval més curt fa el valor més estable, però costa més CPU.

---

## 6. Sessió i tancament del joc

[`SessionState`](../src-tauri/src/session.rs) guarda el joc adjuntat (només un alhora):

1. **Adjuntar:** `OpenProcess` → llistar mòduls → comprovar anti-cheat → engegar el
   `Freezer` i un fil **vigilant**.
2. **Vigilant:** `WaitForSingleObject(handle, 250 ms)` en bucle. Quan el joc es tanca, para
   la congelació, buida la sessió i emet `session-state = detached` cap a la UI.
3. **Desadjuntar:** en eliminar la `Session`, el `Drop` atura els dos fils i tanca el handle.

### Anti-cheat ([`anticheat.rs`](../src-tauri/src/anticheat.rs))

| Situació | Resultat |
|---|---|
| Mòdul d'anti-cheat **dins del joc** (EasyAntiCheat, BEClient…) | **Bloquejat** |
| Anti-cheat en marxa **al sistema** (Vanguard, BEService…) | **Avís**: cal confirmar |

---

## 7. Proves sense jocs reals

La trait [`Memory`](../src-tauri/src/memory/mod.rs) té dues implementacions: `Process` (real) i
`FakeMemory` (un `Vec<u8>` amb adreça base). Així els pointer chains, l'AOB, les validacions
i la congelació es proven amb `cargo test` en mil·lisegons.

El test [`tests/test_game.rs`](../src-tauri/tests/test_game.rs) fa el recorregut complet
amb el joc de prova real: el compila, l'obre, troba la vida per AOB, l'escriu, la congela,
comprova que el joc veu els canvis i detecta quan es tanca.

---

## 8. Motor de trucs ([`trainers/engine.rs`](../src-tauri/src/trainers/engine.rs))

En adjuntar-se a un joc amb definició:

1. **Versió:** es calcula l'SHA-256 de l'executable (amb memòria cau per ruta + mida + data)
   i es compara amb `versions`: verificada, no verificada o sense informació.
2. **Punts de partida:** per a cada truc es resol el punt de partida (`offset` o `aob`) i es
   guarda. L'AOB és la part cara; el resultat no canvia mentre el joc és obert.
3. **A cada ús** es torna a recórrer la cadena de punters, perquè l'objecte pot haver canviat
   d'adreça (nova partida, nou nivell).

| `kind` | Acció | Què fa |
|---|---|---|
| `toggle` + `freeze` | activar | Desa el valor original, escriu i congela |
| `toggle` + `write` | activar | Desa l'original i escriu un cop |
| `toggle` + `patch` | activar | Llegeix els bytes: si no són `expect`, **no escriu res**. Si ho són, desa'ls i escriu el patch |
| `toggle` | desactivar | Deixa de congelar i, amb `off: "restore"` (per defecte), restaura l'original |
| `slider` | posar valor | Comprova `min`/`max`, escriu i, amb `freeze: true`, congela |
| `value` | posar valor | Comprova el rang i escriu un cop |
| `button` | prémer | `add`: llegeix, suma i escriu; `set`: escriu |

En **desadjuntar**, tots els toggles actius es desactiven (restaurant valors i codi). Si el
joc s'ha tancat, no es fa res, perquè no hi ha memòria a restaurar.

La **congelació segueix la cadena de punters**: a cada cicle el `Freezer` recorre de nou els
offsets des del punt de partida. Si un punter és nul (p. ex. al menú principal), es salta
aquell cicle sense comptar-lo com a error.

## 9. Definicions i validació ([`trainers/model.rs`](../src-tauri/src/trainers/model.rs))

- `serde` llegeix el JSON amb `deny_unknown_fields`: un camp mal escrit (`"ofset"`) és un error.
- `validate` comprova la coherència entre camps (un `patch` necessita `bytes` i `expect` de la
  mateixa mida, un slider necessita `min < max`…). Retorna **tots** els errors amb la ruta del
  camp.
- `schemars` genera [schemas/trainer.schema.json](../schemas/trainer.schema.json) a partir dels
  mateixos structs, i `ts-rs` els tipus de TypeScript. Una sola font de veritat.
- Es vigilen les carpetes amb `notify`: en desar un `.json`, el catàleg es recarrega (300 ms
  d'espera per agrupar escriptures) i s'emet `trainers-changed`.

## 10. Dreceres globals ([`hotkeys.rs`](../src-tauri/src/hotkeys.rs))

- Fan servir `tauri-plugin-global-shortcut` (`RegisterHotKey` de Windows), que funciona encara
  que el joc tingui el focus.
- **Només es registren mentre hi ha un joc adjuntat** i es desregistren en desadjuntar.
- Drecera efectiva = la de l'usuari (`settings.json`) o, si no n'hi ha, la de la definició. Una
  drecera buida la desactiva.
- Si un altre programa ja fa servir la combinació, el registre falla i la UI ho mostra al truc.
