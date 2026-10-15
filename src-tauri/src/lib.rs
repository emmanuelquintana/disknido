//! Tauri command layer. Thin: validates inputs, runs engine calls off the UI
//! thread and streams progress as `task://progress` / `task://done` events.
//! Every mutating command goes through the engine's safety checks.

use disknido_core::rusqlite::Connection;
use disknido_core::{
    Control, Error, backups, cleanup, compress, db, duplicates, images, ops, organize, query, recycle, safety, scan, volumes,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

type CmdResult<T> = Result<T, Error>;

struct AppState {
    db_path: PathBuf,
    data_dir: PathBuf,
    conn: Mutex<Connection>,
    tasks: Mutex<HashMap<String, Arc<Control>>>,
    seq: AtomicU64,
    /// The index is being rewritten (scan) — index readers must wait.
    scanning: Arc<AtomicBool>,
}

impl AppState {
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TaskEvent<T: Serialize> {
    task_id: String,
    kind: &'static str,
    data: T,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    task_id: String,
    kind: &'static str,
    ok: bool,
    cancelled: bool,
    result: Value,
    error: Option<String>,
}

/// Spawns a blocking task with its own cancel/pause control and emits
/// throttled progress events plus one `task://done` event.
fn spawn_task<R: Serialize + Send + 'static>(
    app: &AppHandle,
    state: &AppState,
    kind: &'static str,
    job: impl FnOnce(&Control, &(dyn Fn(Value) + Sync)) -> Result<R, Error> + Send + 'static,
) -> String {
    let id = format!("{kind}-{}", state.seq.fetch_add(1, Ordering::Relaxed) + 1);
    let control = Arc::new(Control::default());
    state.tasks.lock().unwrap().insert(id.clone(), control.clone());
    let app = app.clone();
    let task_id = id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let emit = |data: Value| {
            let mut l = last.lock().unwrap();
            if l.elapsed() >= Duration::from_millis(120) {
                *l = Instant::now();
                let _ = app.emit("task://progress", TaskEvent { task_id: task_id.clone(), kind, data });
            }
        };
        let res = job(&control, &emit);
        let (ok, cancelled, result, error) = match res {
            Ok(r) => (true, false, serde_json::to_value(r).unwrap_or(Value::Null), None),
            Err(Error::Cancelled) => (false, true, Value::Null, None),
            Err(e) => (false, false, Value::Null, Some(e.to_string())),
        };
        let _ = app.emit("task://done", DoneEvent { task_id: task_id.clone(), kind, ok, cancelled, result, error });
        if let Some(st) = app.try_state::<AppState>() {
            st.tasks.lock().unwrap().remove(&task_id);
        }
    });
    id
}

fn ensure_index_idle(state: &AppState) -> CmdResult<()> {
    if state.scanning.load(Ordering::SeqCst) {
        return Err(Error::Refused("Hay un análisis en curso; espera a que termine".into()));
    }
    Ok(())
}

// ---------- tasks ----------

#[tauri::command]
fn cancel_task(state: State<AppState>, task_id: String) {
    if let Some(c) = state.tasks.lock().unwrap().get(&task_id) {
        c.cancel();
    }
}

#[tauri::command]
fn pause_task(state: State<AppState>, task_id: String, paused: bool) {
    if let Some(c) = state.tasks.lock().unwrap().get(&task_id) {
        c.set_paused(paused);
    }
}

// ---------- volumes & scan ----------

#[tauri::command]
async fn list_volumes() -> Vec<volumes::Volume> {
    tauri::async_runtime::spawn_blocking(volumes::list_volumes).await.unwrap_or_default()
}

#[tauri::command]
fn start_scan(app: AppHandle, state: State<AppState>, request: scan::ScanRequest) -> CmdResult<String> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err(Error::Refused("Ya hay un análisis en curso".into()));
    }
    let db_path = state.db_path.clone();
    let scanning = state.scanning.clone();
    Ok(spawn_task(&app, &state, "scan", move |control, emit| {
        let res = scan::run_scan(&db_path, &request, control, &|p| emit(serde_json::to_value(p).unwrap_or_default()));
        scanning.store(false, Ordering::SeqCst);
        res
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Overview {
    last_scan: Option<query::ScanInfo>,
    roots: Vec<query::IndexedRoot>,
    categories: Vec<query::Bucket>,
    extensions: Vec<query::Bucket>,
    duplicate_groups: u64,
    duplicate_bytes: u64,
    backups: usize,
    backup_bytes: u64,
    images: usize,
    image_bytes: u64,
    old_folders: usize,
    old_folder_bytes: u64,
    recycle_bin: recycle::BinInfo,
}

#[tauri::command]
async fn overview(state: State<'_, AppState>, root_name: String) -> CmdResult<Overview> {
    ensure_index_idle(&state)?;
    let conn = state.conn();
    let (duplicate_groups, duplicate_bytes) = duplicates::totals(&conn)?;
    let b = backups::list(&conn, &root_name)?;
    let im = images::list(&conn, false)?;
    let old = query::old_folders(&conn, 12, 100 * 1024 * 1024, 100_000, false)?;
    Ok(Overview {
        last_scan: query::last_scan(&conn)?,
        roots: query::indexed_roots(&conn)?,
        categories: query::categories(&conn)?,
        extensions: query::extensions(&conn, 12)?,
        duplicate_groups,
        duplicate_bytes,
        backups: b.len(),
        backup_bytes: b.iter().map(|x| x.size).sum(),
        images: im.len(),
        image_bytes: im.iter().map(|x| x.size).sum(),
        old_folders: old.len(),
        old_folder_bytes: old.iter().map(|x| x.entry.size).sum(),
        recycle_bin: recycle::bin_info(),
    })
}

#[tauri::command]
async fn children(
    state: State<'_, AppState>,
    parent_id: i64,
    offset: i64,
    limit: i64,
) -> CmdResult<(Vec<query::EntryView>, i64)> {
    ensure_index_idle(&state)?;
    query::children(&state.conn(), parent_id, offset.max(0), limit.clamp(1, 2000))
}

#[tauri::command]
async fn largest(state: State<'_, AppState>, limit: i64) -> CmdResult<(Vec<query::EntryView>, Vec<query::EntryView>)> {
    ensure_index_idle(&state)?;
    let conn = state.conn();
    Ok((query::largest_files(&conn, limit.clamp(1, 1000))?, query::largest_folders(&conn, limit.clamp(1, 1000))?))
}

#[tauri::command]
async fn search(state: State<'_, AppState>, needle: String, limit: i64) -> CmdResult<Vec<query::EntryView>> {
    ensure_index_idle(&state)?;
    if needle.trim().len() < 2 {
        return Ok(Vec::new());
    }
    query::search(&state.conn(), needle.trim(), limit.clamp(1, 500))
}

#[tauri::command]
async fn subtree_extensions(state: State<'_, AppState>, id: i64) -> CmdResult<Vec<query::Bucket>> {
    ensure_index_idle(&state)?;
    query::subtree_extensions(&state.conn(), id, 8)
}

#[tauri::command]
async fn scan_errors(state: State<'_, AppState>, scan_id: i64) -> CmdResult<Vec<query::ScanError>> {
    query::scan_errors(&state.conn(), scan_id, 500)
}

#[tauri::command]
async fn old_folders(state: State<'_, AppState>, months: i64, min_bytes: u64) -> CmdResult<Vec<query::OldFolder>> {
    ensure_index_idle(&state)?;
    query::old_folders(&state.conn(), months.clamp(1, 600), min_bytes, 500, true)
}

#[tauri::command]
fn set_mark(state: State<AppState>, path: PathBuf, mark: Option<String>) -> CmdResult<()> {
    if let Some(m) = &mark
        && !["important", "ignored", "favorite", "excluded"].contains(&m.as_str())
    {
        return Err(Error::Refused("Marca no válida".into()));
    }
    safety::check_lexical(&path)?;
    query::set_mark(&state.conn(), &path, mark.as_deref())
}

// ---------- duplicates ----------

#[tauri::command]
fn start_duplicates(app: AppHandle, state: State<AppState>, request: duplicates::DupRequest) -> CmdResult<String> {
    ensure_index_idle(&state)?;
    let db_path = state.db_path.clone();
    Ok(spawn_task(&app, &state, "duplicates", move |control, emit| {
        duplicates::find_duplicates(&db_path, &request, control, &|p| emit(serde_json::to_value(p).unwrap_or_default()))
    }))
}

#[tauri::command]
async fn duplicate_groups(state: State<'_, AppState>, offset: i64, limit: i64) -> CmdResult<(Vec<duplicates::DupGroup>, i64)> {
    duplicates::list_groups(&state.conn(), offset.max(0), limit.clamp(1, 500))
}

#[tauri::command]
async fn recycle_duplicates(
    state: State<'_, AppState>,
    group_id: i64,
    keep: PathBuf,
    remove: Vec<PathBuf>,
) -> CmdResult<recycle::RecycleResult> {
    let conn = state.conn();
    recycle::recycle_duplicates(&conn, group_id, &keep, &remove, &Control::default())
}

fn check_export_path(path: &std::path::Path) -> CmdResult<()> {
    safety::check_lexical(path)?;
    if let Some(r) = safety::protected_reason(path, false) {
        return Err(Error::Refused(format!("Ubicación protegida ({r})")));
    }
    safety::check_no_links(path)
}

#[tauri::command]
async fn export_duplicates(state: State<'_, AppState>, path: PathBuf, anonymize: bool) -> CmdResult<()> {
    check_export_path(&path)?;
    let csv = duplicates::export_csv(&state.conn(), anonymize)?;
    std::fs::write(&path, csv)?;
    Ok(())
}

// ---------- disk images ----------

#[tauri::command]
async fn disk_images(state: State<'_, AppState>, read_headers: bool) -> CmdResult<Vec<images::DiskImage>> {
    ensure_index_idle(&state)?;
    images::list(&state.conn(), read_headers)
}

#[tauri::command]
async fn image_orphans(state: State<'_, AppState>, selection: Vec<PathBuf>) -> CmdResult<Vec<String>> {
    let all = images::list(&state.conn(), false)?;
    Ok(images::orphaned_by(&selection, &all))
}

#[tauri::command]
async fn recycle_images(state: State<'_, AppState>, targets: Vec<recycle::RecycleTarget>) -> CmdResult<recycle::RecycleResult> {
    for t in &targets {
        let ext = t.path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !disknido_core::classify::DISK_IMAGE_EXTS.contains(&ext.as_str()) && ext != "sub" {
            return Err(Error::Refused(format!("{} no es una imagen de disco", t.path.display())));
        }
    }
    recycle::recycle(&state.conn(), "images", &targets, false, &Control::default(), &|_| Ok(()))
}

// ---------- backups ----------

#[tauri::command]
async fn backups_list(state: State<'_, AppState>, root_name: String) -> CmdResult<Vec<backups::BackupItem>> {
    ensure_index_idle(&state)?;
    backups::list(&state.conn(), &root_name)
}

#[tauri::command]
fn backup_override(state: State<AppState>, path: PathBuf, value: backups::Override) -> CmdResult<()> {
    safety::check_lexical(&path)?;
    backups::set_override(&state.conn(), &path, &value)
}

#[tauri::command]
async fn organize_plan(request: organize::PlanRequest) -> CmdResult<organize::Plan> {
    tauri::async_runtime::spawn_blocking(move || organize::plan(&request, &Control::default()))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
fn organize_execute(app: AppHandle, state: State<AppState>, items: Vec<organize::PlanItem>) -> CmdResult<String> {
    let db_path = state.db_path.clone();
    Ok(spawn_task(&app, &state, "organize", move |control, emit| {
        let conn = db::open(&db_path)?;
        organize::execute(&conn, &items, control, &|i, n| emit(serde_json::json!({ "done": i, "total": n })))
    }))
}

#[tauri::command]
async fn organize_undo(state: State<'_, AppState>, op_id: i64) -> CmdResult<organize::ExecResult> {
    organize::undo(&state.conn(), op_id, &Control::default())
}

// ---------- compression ----------

#[tauri::command]
async fn compress_preview(request: compress::CompressRequest) -> CmdResult<compress::Preview> {
    tauri::async_runtime::spawn_blocking(move || compress::preview(&request, &Control::default()))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
fn start_compress(app: AppHandle, state: State<AppState>, request: compress::CompressRequest) -> CmdResult<String> {
    let db_path = state.db_path.clone();
    Ok(spawn_task(&app, &state, "compress", move |control, emit| {
        let conn = db::open(&db_path)?;
        compress::compress(&conn, &request, control, &|p| emit(serde_json::to_value(p).unwrap_or_default()))
    }))
}

// ---------- cleanup ----------

#[tauri::command]
async fn cleanup_analyze() -> CmdResult<Vec<cleanup::CleanupCategory>> {
    tauri::async_runtime::spawn_blocking(|| cleanup::analyze(&Control::default()))
        .await
        .map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
async fn cleanup_recycle(state: State<'_, AppState>, rule_id: String, paths: Vec<PathBuf>) -> CmdResult<recycle::RecycleResult> {
    let targets = cleanup::check_rule_paths(&rule_id, &paths)?;
    let appdata = cleanup::rule_allows_appdata(&rule_id);
    recycle::recycle(&state.conn(), &rule_id, &targets, appdata, &Control::default(), &|_| Ok(()))
}

#[tauri::command]
async fn empty_recycle_bin(state: State<'_, AppState>) -> CmdResult<recycle::BinInfo> {
    recycle::empty_bin(&state.conn())
}

// ---------- history, settings, misc ----------

#[tauri::command]
async fn history(state: State<'_, AppState>, limit: i64) -> CmdResult<Vec<ops::Operation>> {
    ops::history(&state.conn(), limit.clamp(1, 500))
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> CmdResult<Option<Value>> {
    Ok(db::get_setting(&state.conn(), "app")?.and_then(|s| serde_json::from_str(&s).ok()))
}

#[tauri::command]
fn set_settings(state: State<AppState>, value: Value) -> CmdResult<()> {
    let s = value.to_string();
    if s.len() > 256 * 1024 {
        return Err(Error::Refused("Configuración demasiado grande".into()));
    }
    db::set_setting(&state.conn(), "app", &s)?;
    Ok(())
}

#[tauri::command]
fn reset_index(state: State<AppState>) -> CmdResult<()> {
    ensure_index_idle(&state)?;
    db::reset_index(&state.conn())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    data_dir: String,
    db_bytes: u64,
    schema_version: i64,
}

#[tauri::command]
fn app_info(state: State<AppState>) -> CmdResult<AppInfo> {
    let db_bytes = ["", "-wal", "-shm"]
        .iter()
        .filter_map(|s| std::fs::metadata(format!("{}{s}", state.db_path.display())).ok())
        .map(|m| m.len())
        .sum();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        db_bytes,
        schema_version: db::schema_version(&state.conn())?,
    })
}

#[tauri::command]
fn reveal_path(path: PathBuf) -> CmdResult<()> {
    safety::check_lexical(&path)?;
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| Error::Other(e.to_string()))
}

#[tauri::command]
fn open_storage_settings() -> CmdResult<()> {
    tauri_plugin_opener::open_url("ms-settings:storagesense", None::<&str>).map_err(|e| Error::Other(e.to_string()))
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            safety::set_app_data_dir(data_dir.clone());
            let db_path = data_dir.join("index.sqlite");
            let conn = db::open(&db_path).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let _ = ops::reconcile_interrupted(&conn);
            let _ = db::recover_interrupted(&conn);
            app.manage(AppState {
                db_path,
                data_dir,
                conn: Mutex::new(conn),
                tasks: Mutex::new(HashMap::new()),
                seq: AtomicU64::new(0),
                scanning: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cancel_task,
            pause_task,
            list_volumes,
            start_scan,
            overview,
            children,
            largest,
            search,
            subtree_extensions,
            scan_errors,
            old_folders,
            set_mark,
            start_duplicates,
            duplicate_groups,
            recycle_duplicates,
            export_duplicates,
            disk_images,
            image_orphans,
            recycle_images,
            backups_list,
            backup_override,
            organize_plan,
            organize_execute,
            organize_undo,
            compress_preview,
            start_compress,
            cleanup_analyze,
            cleanup_recycle,
            empty_recycle_bin,
            history,
            get_settings,
            set_settings,
            reset_index,
            app_info,
            reveal_path,
            open_storage_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DiskNido");
}
