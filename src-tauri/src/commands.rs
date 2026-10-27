//! Comandes que el frontend pot cridar amb `invoke`. Han de ser primes: validen
//! l'entrada i deleguen als mòduls. Les `async` s'executen fora del fil principal.

use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;

use crate::error::{AppError, AppResult};
use crate::games::{GameEntry, GameStore, RunningGame};
use crate::hotkeys;
use crate::logging::{self, LogEntry, LogLevel};
use crate::memory::locator::{self, Locator, Offset};
use crate::memory::scanner::{self, PointerCandidate, ScanFilter, ScanHit, ScanSummary};
use crate::memory::process::{self, ProcessInfo};
use crate::memory::value::ValueType;
use crate::memory::{Memory, WriteMode};
use crate::session::{Session, SessionInfo, SessionState};
use crate::settings::{ManualGame, Settings, SettingsStore};
use crate::trainers::engine::{CheatInput, CheatState, VersionInfo};
use crate::trainers::model::{CheatDef, CheatKind, valid_hotkey};
use crate::trainers::{CatalogView, TrainerStore, editor, exe_sha256};

#[tauri::command]
pub async fn list_processes() -> AppResult<Vec<ProcessInfo>> {
    let list = process::list_processes()?;
    tracing::debug!(count = list.len(), "processos llistats");
    Ok(list)
}

#[tauri::command]
pub fn get_logs(limit: Option<usize>, min_level: Option<LogLevel>) -> Vec<LogEntry> {
    logging::recent(limit.unwrap_or(500).min(2000), min_level.unwrap_or(LogLevel::Trace))
}

// --- Sessió ---

#[tauri::command]
pub async fn attach(
    app: AppHandle,
    state: State<'_, SessionState>,
    pid: u32,
    acknowledge_warnings: Option<bool>,
) -> AppResult<SessionInfo> {
    state.attach(&app, pid, acknowledge_warnings.unwrap_or(false))
}

#[tauri::command]
pub fn detach(app: AppHandle, state: State<'_, SessionState>) -> bool {
    state.detach(&app, "Desadjuntat per l'usuari", None, true)
}

#[tauri::command]
pub fn session_info(state: State<'_, SessionState>) -> Option<SessionInfo> {
    state.info()
}

// --- Trainers ---

#[tauri::command]
pub fn list_trainers(store: State<'_, TrainerStore>) -> CatalogView {
    store.view()
}

#[tauri::command]
pub fn reload_trainers(store: State<'_, TrainerStore>, games: State<'_, GameStore>) -> CatalogView {
    store.reload();
    games.rematch(&store.catalog.read().unwrap_or_else(|e| e.into_inner()));
    store.view()
}

// --- Biblioteca ---

/// Jocs instal·lats. La primera crida (o amb `refresh`) escaneja les botigues.
#[tauri::command]
pub async fn list_games(app: AppHandle, games: State<'_, GameStore>, refresh: Option<bool>) -> AppResult<Vec<GameEntry>> {
    Ok(games.installed(&app, refresh.unwrap_or(false)))
}

#[tauri::command]
pub fn running_games(games: State<'_, GameStore>) -> Vec<RunningGame> {
    games.running()
}

/// Obre el selector de fitxers i afegeix l'executable triat com a joc manual.
#[tauri::command]
pub async fn add_manual_game(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
    games: State<'_, GameStore>,
) -> AppResult<Option<Vec<GameEntry>>> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().set_title("Tria l'executable del joc").add_filter("Executable", &["exe"]).blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    if !path.is_file() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
        return Err(AppError::InvalidValue("cal triar un fitxer .exe".into()));
    }
    let path_str = path.display().to_string();
    let mut s = settings.get();
    if !s.manual_games.iter().any(|g| g.path.eq_ignore_ascii_case(&path_str)) {
        let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        s.manual_games.push(ManualGame { name, path: path_str.clone() });
        settings.save(s)?;
        tracing::info!(path = %path_str, "joc afegit a mà");
    }
    Ok(Some(games.installed(&app, true)))
}

#[tauri::command]
pub async fn remove_manual_game(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
    games: State<'_, GameStore>,
    path: String,
) -> AppResult<Vec<GameEntry>> {
    let mut s = settings.get();
    s.manual_games.retain(|g| !g.path.eq_ignore_ascii_case(&path));
    settings.save(s)?;
    Ok(games.installed(&app, true))
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheatView {
    pub def: CheatDef,
    pub state: CheatState,
    /// Dreceres efectives (la de l'usuari o la de la definició)
    pub hotkey: Option<String>,
    pub hotkey_increase: Option<String>,
    pub hotkey_decrease: Option<String>,
    pub hotkey_error: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrainerView {
    pub id: String,
    pub name: String,
    pub version: VersionInfo,
    pub cheats: Vec<CheatView>,
}

#[tauri::command]
pub async fn trainer_view(state: State<'_, SessionState>, settings: State<'_, SettingsStore>) -> AppResult<Option<TrainerView>> {
    let overrides = settings.get().hotkey_overrides;
    let result = state.with_trainer(|s, t| {
        let states = t.refresh(s.process.as_ref(), &s.modules);
        let errors = s.hotkey_errors.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let o = overrides.get(&t.def.id);
        let cheats = t
            .def
            .cheats
            .iter()
            .zip(states)
            .map(|(c, state)| {
                let hk = c.hotkeys.clone().unwrap_or_default();
                let err_for = |k: &str| errors.iter().find(|(key, _)| key == k).map(|(_, e)| e.clone());
                CheatView {
                    hotkey: hotkeys::effective(o, &c.id, c.hotkey.as_ref()),
                    hotkey_increase: hotkeys::effective(o, &format!("{}:increase", c.id), hk.increase.as_ref()),
                    hotkey_decrease: hotkeys::effective(o, &format!("{}:decrease", c.id), hk.decrease.as_ref()),
                    hotkey_error: err_for(&c.id)
                        .or_else(|| err_for(&format!("{}:increase", c.id)))
                        .or_else(|| err_for(&format!("{}:decrease", c.id))),
                    def: c.clone(),
                    state,
                }
            })
            .collect();
        Ok(TrainerView { id: t.def.id.clone(), name: t.def.name.clone(), version: t.version.clone(), cheats })
    });
    match result {
        Ok(v) => Ok(Some(v)),
        Err(AppError::NotAttached | AppError::InvalidValue(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn cheat_states(state: State<'_, SessionState>) -> AppResult<Vec<CheatState>> {
    state.with_trainer(|s, t| Ok(t.refresh(s.process.as_ref(), &s.modules)))
}

#[tauri::command]
pub async fn set_cheat(state: State<'_, SessionState>, cheat_id: String, input: CheatInput) -> AppResult<CheatState> {
    state.with_trainer(|s, t| t.apply(&cheat_id, input, s.process.as_ref(), &s.modules, &s.freezer))
}

/// Canvia la drecera d'un truc del joc adjuntat. `key` = id del truc, o `id:increase` /
/// `id:decrease` als sliders. `accelerator` buit desactiva la drecera; `null` torna a la
/// de la definició.
#[tauri::command]
pub fn set_hotkey(
    app: AppHandle,
    state: State<'_, SessionState>,
    settings: State<'_, SettingsStore>,
    key: String,
    accelerator: Option<String>,
) -> AppResult<()> {
    let def = state.with_trainer(|_, t| Ok(t.def.clone()))?;
    let cheat_id = key.split(':').next().unwrap_or_default();
    let cheat = def.cheats.iter().find(|c| c.id == cheat_id).ok_or_else(|| AppError::InvalidValue(format!("no hi ha cap truc «{cheat_id}»")))?;
    let suffix_ok = match key.split_once(':') {
        None => matches!(cheat.kind, CheatKind::Toggle | CheatKind::Button),
        Some((_, s)) => cheat.kind == CheatKind::Slider && matches!(s, "increase" | "decrease"),
    };
    if !suffix_ok {
        return Err(AppError::InvalidValue(format!("«{key}» no admet drecera")));
    }
    if let Some(a) = &accelerator
        && !a.is_empty()
        && !valid_hotkey(a)
    {
        return Err(AppError::InvalidValue(format!("drecera «{a}» no vàlida")));
    }

    let mut s = settings.get();
    let entry = s.hotkey_overrides.entry(def.id.clone()).or_default();
    match accelerator {
        Some(a) => entry.insert(key.clone(), a),
        None => entry.remove(&key),
    };
    settings.save(s)?;
    state.set_hotkey_errors(hotkeys::register(&app, &def));
    tracing::info!(trainer = %def.id, key, "drecera actualitzada");
    Ok(())
}

// --- Configuració ---

#[tauri::command]
pub fn get_settings(settings: State<'_, SettingsStore>) -> Settings {
    settings.get()
}

#[tauri::command]
pub fn save_settings(state: State<'_, SessionState>, settings: State<'_, SettingsStore>, value: Settings) -> AppResult<()> {
    let interval = value.freeze_interval_ms;
    settings.save(value)?;
    let _ = state.with(|s| {
        s.freezer.set_interval(interval);
        Ok(())
    });
    Ok(())
}

// --- Escàner (Fase 5) ---

fn summary(s: &scanner::ScanState, started: std::time::Instant) -> ScanSummary {
    ScanSummary { count: s.count(), truncated: s.truncated, scans: s.scans, elapsed_ms: started.elapsed().as_millis() as u64 }
}

#[tauri::command]
pub async fn scan_first(state: State<'_, SessionState>, value_type: ValueType, value: f64) -> AppResult<ScanSummary> {
    if !value_type.is_numeric() {
        return Err(AppError::InvalidValue("l'escàner només busca valors numèrics".into()));
    }
    value_type.encode(value)?; // valida el rang abans d'escanejar
    state.with(|s| {
        let started = std::time::Instant::now();
        let scan = scanner::first_scan(s.process.as_ref(), value_type, value);
        let sum = summary(&scan, started);
        tracing::info!(count = sum.count, ms = sum.elapsed_ms, value, ty = ?value_type, "primera cerca");
        *s.scan.lock().unwrap_or_else(|e| e.into_inner()) = Some(scan);
        Ok(sum)
    })
}

#[tauri::command]
pub async fn scan_next(state: State<'_, SessionState>, filter: ScanFilter) -> AppResult<ScanSummary> {
    state.with(|s| {
        let mut guard = s.scan.lock().unwrap_or_else(|e| e.into_inner());
        let scan = guard.as_mut().ok_or_else(|| AppError::InvalidValue("primer fes una primera cerca".into()))?;
        let started = std::time::Instant::now();
        scan.next_scan(s.process.as_ref(), filter);
        let sum = summary(scan, started);
        tracing::info!(count = sum.count, ms = sum.elapsed_ms, filter = ?filter, "cerca següent");
        Ok(sum)
    })
}

#[tauri::command]
pub async fn scan_results(state: State<'_, SessionState>, offset: usize, limit: usize) -> AppResult<Vec<ScanHit>> {
    state.with(|s| {
        let guard = s.scan.lock().unwrap_or_else(|e| e.into_inner());
        Ok(guard.as_ref().map(|scan| scan.page(s.process.as_ref(), &s.modules, offset, limit.min(500))).unwrap_or_default())
    })
}

#[tauri::command]
pub fn scan_reset(state: State<'_, SessionState>) -> AppResult<()> {
    state.with(|s| {
        *s.scan.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    })
}

fn parse_address(address: &str) -> AppResult<u64> {
    Offset::parse(address)
        .filter(|a| *a > 0)
        .map(|a| a as u64)
        .ok_or_else(|| AppError::InvalidValue(format!("adreça invàlida «{address}»")))
}

/// Escriu un valor a una adreça trobada amb l'escàner (passa per les validacions d'escriptura).
#[tauri::command]
pub async fn scan_write(state: State<'_, SessionState>, address: String, value_type: ValueType, value: f64) -> AppResult<MemoryReading> {
    let address = parse_address(&address)?;
    let bytes = value_type.encode(value)?;
    state.with(|s| {
        s.process.write_bytes(address, &bytes, WriteMode::Data)?;
        let mut b = vec![0u8; value_type.size()];
        s.process.read_bytes(address, &mut b)?;
        tracing::info!(address = format_args!("{address:#x}"), value, "valor escrit des de l'escàner");
        Ok(MemoryReading { address: format!("{address:#x}"), value: value_type.decode(&b)? })
    })
}

/// Busca variables globals que apuntin a l'adreça (o just abans): candidats per a un truc
/// que continuï funcionant després de reiniciar el joc.
#[tauri::command]
pub async fn find_pointers(state: State<'_, SessionState>, address: String) -> AppResult<Vec<PointerCandidate>> {
    let target = parse_address(&address)?;
    state.with(|s| Ok(scanner::find_static_pointers(s.process.as_ref(), &s.modules, target, 0x1000, 20)))
}

/// Afegeix un truc a la definició del joc adjuntat (en crea una si cal) i la recarrega.
/// Retorna la ruta del fitxer desat.
#[tauri::command]
pub async fn save_cheat(
    app: AppHandle,
    state: State<'_, SessionState>,
    store: State<'_, TrainerStore>,
    games: State<'_, GameStore>,
    cheat: CheatDef,
) -> AppResult<String> {
    let (exe, arch, path) = state.with(|s| Ok((s.process.name.clone(), s.process.arch, s.process.path.clone())))?;
    let sha = exe_sha256(std::path::Path::new(&path)).unwrap_or_default();
    let (def, file) = editor::add_cheat(&store, &exe, arch, &sha, cheat)?;
    games.rematch(&store.catalog.read().unwrap_or_else(|e| e.into_inner()));
    state.reload_trainer(&app)?;
    let _ = tauri::Emitter::emit(&app, "trainers-changed", ());
    tracing::info!(trainer = %def.id, "definició desada");
    Ok(file.display().to_string())
}

// --- Depuració (Fase 2): llegir/escriure qualsevol localitzador ---

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryReading {
    /// Adreça resolta, en hex
    pub address: String,
    pub value: f64,
}

fn resolve_and_read(s: &Session, loc: &Locator, ty: ValueType) -> AppResult<(u64, f64)> {
    let address = locator::resolve(s.process.as_ref(), &s.modules, loc)?;
    let mut buf = vec![0u8; ty.size()];
    s.process.read_bytes(address, &mut buf)?;
    Ok((address, ty.decode(&buf)?))
}

#[tauri::command]
pub async fn debug_read(state: State<'_, SessionState>, locator: Locator, value_type: ValueType) -> AppResult<MemoryReading> {
    state.with(|s| {
        let (address, value) = resolve_and_read(s, &locator, value_type)?;
        Ok(MemoryReading { address: format!("{address:#x}"), value })
    })
}

#[tauri::command]
pub async fn debug_write(
    state: State<'_, SessionState>,
    locator: Locator,
    value_type: ValueType,
    value: f64,
    freeze: bool,
) -> AppResult<MemoryReading> {
    let bytes = value_type.encode(value)?;
    state.with(|s| {
        let address = locator::resolve(s.process.as_ref(), &s.modules, &locator)?;
        s.process.write_bytes(address, &bytes, WriteMode::Data)?;
        if freeze {
            s.freezer.set(&format!("{address:#x}:{value_type:?}"), address, bytes);
        }
        tracing::info!(address = format_args!("{address:#x}"), value, freeze, "valor escrit");
        let (_, value) = resolve_and_read(s, &locator, value_type)?;
        Ok(MemoryReading { address: format!("{address:#x}"), value })
    })
}

#[tauri::command]
pub fn list_frozen(state: State<'_, SessionState>) -> AppResult<Vec<String>> {
    state.with(|s| Ok(s.freezer.keys()))
}

#[tauri::command]
pub fn unfreeze(state: State<'_, SessionState>, key: String) -> AppResult<bool> {
    state.with(|s| Ok(s.freezer.remove(&key)))
}
