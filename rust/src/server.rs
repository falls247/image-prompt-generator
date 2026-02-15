use anyhow::{anyhow, Context, Result};
use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

use crate::config_store::{ConfigStore, ItemConfig};
use crate::history_store::HistoryStore;
use crate::main_ui_html::build_main_ui_html;
use crate::renderer::{render_prompt, RenderEntry};
use crate::NO_SELECTION;

pub struct AppState {
    pub config: Mutex<ConfigStore>,
    pub history: Mutex<HistoryStore>,
    pub copy_state: Mutex<CopyState>,
    pub server_port: AtomicU16,
    pub history_revision: AtomicU64,
}

type ApiResponse = (StatusCode, Json<Value>);

pub struct CopyState {
    pub last_prompt: String,
    pub last_copy_time: Option<Instant>,
}

impl AppState {
    pub fn new(config: ConfigStore, history: HistoryStore) -> Self {
        Self {
            config: Mutex::new(config),
            history: Mutex::new(history),
            copy_state: Mutex::new(CopyState {
                last_prompt: String::new(),
                last_copy_time: None,
            }),
            server_port: AtomicU16::new(0),
            history_revision: AtomicU64::new(0),
        }
    }
}

pub struct AppServer {
    port: u16,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl AppServer {
    pub fn start(state: Arc<AppState>, preferred_port: u16) -> Result<Self> {
        let listener = bind_listener(preferred_port)?;
        let port = listener
            .local_addr()
            .context("failed to inspect server local address")?
            .port();
        listener
            .set_nonblocking(true)
            .context("failed to set listener non-blocking")?;

        state.server_port.store(port, Ordering::Relaxed);

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let thread_handle = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let Ok(runtime) = runtime else {
                return;
            };

            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(_) => return,
                };

                let app = build_router(state);
                let server = axum::serve(listener, app).with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });
                let _ = server.await;
            });
        });

        Ok(Self {
            port,
            shutdown_tx: Some(shutdown_tx),
            thread_handle: Some(thread_handle),
        })
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone, Serialize)]
struct UiRow {
    item_id: String,
    label: String,
    choices: Vec<String>,
    allow_free_text: bool,
    selected: String,
    free_text: String,
}

#[derive(Debug, Clone, Serialize)]
struct UiSnapshot {
    rows: Vec<UiRow>,
    preview: String,
    confirm_delete: bool,
}

#[derive(Debug, Deserialize)]
struct HistoryDeleteReq {
    history_id: String,
}

#[derive(Debug, Deserialize)]
struct HistoryUpdateReq {
    history_id: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct HistoryImageReq {
    path: String,
}

#[derive(Debug, Deserialize)]
struct ComboChangeReq {
    item_id: String,
    selected: String,
}

#[derive(Debug, Deserialize)]
struct FreeConfirmReq {
    item_id: String,
    selected: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct DeleteChoiceReq {
    item_id: String,
    selected: String,
}

#[derive(Debug, Deserialize)]
struct CopyReq {
    prompt: String,
}

fn build_router(state: Arc<AppState>) -> Router {
    let port = state.server_port.load(Ordering::Relaxed);
    let local_origin = HeaderValue::from_str(&format!("http://127.0.0.1:{port}"))
        .expect("127.0.0.1 origin should be valid");
    let localhost_origin = HeaderValue::from_str(&format!("http://localhost:{port}"))
        .expect("localhost origin should be valid");

    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("null"),
            local_origin,
            localhost_origin,
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new()
        .route("/", get(get_main_page))
        .route("/ping", get(get_ping))
        .route("/image", get(get_history_image))
        .route("/delete", post(post_delete_history))
        .route("/update", post(post_update_history))
        .route("/upload", post(post_upload_history))
        .route("/app/init", get(get_app_init))
        .route("/app/history-revision", get(get_app_history_revision))
        .route("/app/combo-change", post(post_app_combo_change))
        .route("/app/free-confirm", post(post_app_free_confirm))
        .route("/app/delete-choice", post(post_app_delete_choice))
        .route("/app/reset", post(post_app_reset))
        .route("/app/copy", post(post_app_copy))
        .route("/app/open-history", post(post_app_open_history))
        .layer(DefaultBodyLimit::max(
            HistoryStore::MAX_IMAGE_BYTES + 200_000,
        ))
        .layer(cors)
        .with_state(state)
}

async fn get_main_page() -> Html<String> {
    Html(build_main_ui_html())
}

async fn get_ping() -> ApiResponse {
    ok_json(json!({}))
}

async fn get_history_image(
    State(state): State<Arc<AppState>>,
    Query(payload): Query<HistoryImageReq>,
) -> axum::response::Response {
    let image_path = payload.path.trim().to_string();
    if image_path.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "path is required").into_response();
    }

    let image = {
        let history = match state.history.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "history store lock error",
                )
                .into_response()
            }
        };

        history.read_image_blob(&image_path)
    };

    match image {
        Ok((bytes, content_type)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
        Err(err) => {
            let message = err.to_string();
            let status = if message.contains("failed to read image") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            err_json(status, &message).into_response()
        }
    }
}

async fn post_delete_history(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<HistoryDeleteReq>,
) -> ApiResponse {
    let history_id = payload.history_id.trim().to_string();
    if history_id.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "history_id is required");
    }

    let port = state.server_port.load(Ordering::Relaxed);
    let removed = {
        let mut history = match state.history.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "history store lock error",
                )
            }
        };

        match history.delete_history(&history_id) {
            Ok(removed) => {
                if !removed {
                    return err_json(StatusCode::NOT_FOUND, "history id not found");
                }
                if let Err(err) = history.regenerate_html(port) {
                    return err_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("delete failed: {err}"),
                    );
                }
                removed
            }
            Err(err) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("delete failed: {err}"),
                )
            }
        }
    };

    if removed {
        ok_json(json!({}))
    } else {
        err_json(StatusCode::NOT_FOUND, "history id not found")
    }
}

async fn post_update_history(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<HistoryUpdateReq>,
) -> ApiResponse {
    let history_id = payload.history_id.trim().to_string();
    if history_id.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "history_id is required");
    }

    let prompt = payload.prompt.trim().to_string();
    if prompt.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "prompt is required");
    }

    let port = state.server_port.load(Ordering::Relaxed);
    let updated = {
        let mut history = match state.history.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "history store lock error",
                )
            }
        };

        match history.update_history_prompt(&history_id, &prompt) {
            Ok(updated) => {
                if !updated {
                    return err_json(StatusCode::NOT_FOUND, "history id not found");
                }
            }
            Err(err) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("update failed: {err}"),
                )
            }
        }

        if let Err(err) = history.regenerate_html(port) {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("update failed: {err}"),
            );
        }

        prompt
    };

    ok_json(json!({ "prompt": updated }))
}

async fn post_upload_history(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> ApiResponse {
    let mut history_id = String::new();
    let mut file_name = String::from("upload.bin");
    let mut file_data = Vec::new();

    loop {
        match multipart.next_field().await {
            Ok(Some(field)) => {
                let field_name = field.name().unwrap_or_default().to_string();
                if field_name == "history_id" {
                    match field.text().await {
                        Ok(value) => history_id = value.trim().to_string(),
                        Err(_) => return err_json(StatusCode::BAD_REQUEST, "invalid history_id"),
                    }
                } else if field_name == "file" {
                    file_name = field
                        .file_name()
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| "upload.bin".to_string());
                    match field.bytes().await {
                        Ok(bytes) => file_data = bytes.to_vec(),
                        Err(_) => return err_json(StatusCode::BAD_REQUEST, "invalid file"),
                    }
                }
            }
            Ok(None) => break,
            Err(_) => return err_json(StatusCode::BAD_REQUEST, "invalid multipart request"),
        }
    }

    if history_id.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "history_id is required");
    }

    if file_data.is_empty() {
        return err_json(StatusCode::BAD_REQUEST, "file is required");
    }

    if file_data.len() > HistoryStore::MAX_IMAGE_BYTES {
        return err_json(StatusCode::BAD_REQUEST, "file size exceeds 20MB");
    }

    let port = state.server_port.load(Ordering::Relaxed);
    let image_path = {
        let mut history = match state.history.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "history store lock error",
                )
            }
        };

        let image_path = match history.append_image(&history_id, &file_name, &file_data) {
            Ok(path) => path,
            Err(err) => {
                let message = err.to_string();
                if message.contains("not found") {
                    return err_json(StatusCode::NOT_FOUND, &message);
                }
                return err_json(StatusCode::BAD_REQUEST, &message);
            }
        };

        if let Err(err) = history.regenerate_html(port) {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("upload failed: {err}"),
            );
        }

        image_path
    };

    ok_json(json!({ "image_path": image_path }))
}

async fn get_app_init(State(state): State<Arc<AppState>>) -> ApiResponse {
    let snapshot = {
        let config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };
        build_ui_snapshot(&config)
    };

    ok_snapshot(snapshot)
}

async fn get_app_history_revision(State(state): State<Arc<AppState>>) -> ApiResponse {
    let revision = state.history_revision.load(Ordering::Relaxed);
    ok_json(json!({ "revision": revision }))
}

async fn post_app_combo_change(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ComboChangeReq>,
) -> ApiResponse {
    let (section, key) = match split_item_id(&payload.item_id) {
        Ok(pair) => pair,
        Err(message) => return err_json(StatusCode::BAD_REQUEST, &message),
    };

    let snapshot = {
        let mut config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };

        let Some(item) = find_item(&config, &section, &key) else {
            return err_json(StatusCode::NOT_FOUND, "item not found");
        };

        let selected = payload.selected.trim();
        let selected_value = if selected.is_empty() || !item.choices.iter().any(|c| c == selected) {
            NO_SELECTION
        } else {
            selected
        };

        if let Err(err) = config.set_item_state(&section, &key, selected_value, "") {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("save error: {err}"),
            );
        }

        build_ui_snapshot(&config)
    };

    ok_snapshot(snapshot)
}

async fn post_app_free_confirm(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FreeConfirmReq>,
) -> ApiResponse {
    let (section, key) = match split_item_id(&payload.item_id) {
        Ok(pair) => pair,
        Err(message) => return err_json(StatusCode::BAD_REQUEST, &message),
    };

    let snapshot = {
        let mut config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };

        let Some(item) = find_item(&config, &section, &key) else {
            return err_json(StatusCode::NOT_FOUND, "item not found");
        };

        let incoming = payload.value.trim().to_string();
        if incoming.is_empty() || incoming == NO_SELECTION {
            let selected = payload.selected.trim();
            let selected_value =
                if selected.is_empty() || !item.choices.iter().any(|c| c == selected) {
                    NO_SELECTION
                } else {
                    selected
                };
            if let Err(err) = config.set_item_state(&section, &key, selected_value, "") {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("save error: {err}"),
                );
            }
        } else {
            if let Err(err) = config.add_choice(&section, &key, &incoming) {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("save error: {err}"),
                );
            }
            if let Err(err) = config.set_item_state(&section, &key, &incoming, &incoming) {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("save error: {err}"),
                );
            }
        }

        build_ui_snapshot(&config)
    };

    ok_snapshot(snapshot)
}

async fn post_app_delete_choice(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeleteChoiceReq>,
) -> ApiResponse {
    let (section, key) = match split_item_id(&payload.item_id) {
        Ok(pair) => pair,
        Err(message) => return err_json(StatusCode::BAD_REQUEST, &message),
    };

    let snapshot = {
        let mut config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };

        let selected = payload.selected.trim();
        if !selected.is_empty() && selected != NO_SELECTION {
            match config.remove_choice(&section, &key, selected) {
                Ok(removed) if removed => {
                    let (_, free_text) = config.get_item_state(&section, &key);
                    let next_free_text = if free_text == selected {
                        String::new()
                    } else {
                        free_text
                    };
                    if let Err(err) =
                        config.set_item_state(&section, &key, NO_SELECTION, &next_free_text)
                    {
                        return err_json(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &format!("save error: {err}"),
                        );
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    return err_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("delete error: {err}"),
                    );
                }
            }
        }

        build_ui_snapshot(&config)
    };

    ok_snapshot(snapshot)
}

async fn post_app_reset(State(state): State<Arc<AppState>>) -> ApiResponse {
    let snapshot = {
        let mut config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };

        if let Err(err) = config.clear_section_state("prompt") {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("save error: {err}"),
            );
        }

        build_ui_snapshot(&config)
    };

    ok_snapshot(snapshot)
}

async fn post_app_copy(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CopyReq>,
) -> ApiResponse {
    let prompt = payload.prompt.trim().to_string();
    if prompt.is_empty() {
        return ok_json(json!({ "skipped": true }));
    }

    let debounce = {
        let config = match state.config.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "config lock error"),
        };
        config.copy_debounce_sec()
    };

    {
        let mut copy_state = match state.copy_state.lock() {
            Ok(guard) => guard,
            Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "copy state lock error"),
        };

        if copy_state.last_prompt == prompt {
            if let Some(last_copy) = copy_state.last_copy_time {
                if last_copy.elapsed().as_secs_f64() <= debounce {
                    return ok_json(json!({ "skipped": true }));
                }
            }
        }

        if let Err(err) = copy_to_system_clipboard(&prompt) {
            return err_json(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("clipboard error: {err}"),
            );
        }

        let port = state.server_port.load(Ordering::Relaxed);
        {
            let mut history = match state.history.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    return err_json(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "history store lock error",
                    )
                }
            };

            if let Err(err) = history.append_history(&prompt) {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("history save error: {err}"),
                );
            }
            if let Err(err) = history.regenerate_html(port) {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("history render error: {err}"),
                );
            }
        }

        copy_state.last_prompt = prompt;
        copy_state.last_copy_time = Some(Instant::now());
        state.history_revision.fetch_add(1, Ordering::Relaxed);
    }

    ok_json(json!({ "skipped": false }))
}

async fn post_app_open_history(State(state): State<Arc<AppState>>) -> ApiResponse {
    let path = {
        let history = match state.history.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return err_json(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "history store lock error",
                )
            }
        };
        history.history_html_path().to_path_buf()
    };

    if !path.exists() {
        return err_json(
            StatusCode::NOT_FOUND,
            &format!("History.html not found: {}", path.display()),
        );
    }

    if let Err(err) = open_file_in_browser(&path) {
        return err_json(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("open history failed: {err}"),
        );
    }

    ok_json(json!({}))
}

fn ok_json(payload: Value) -> ApiResponse {
    let mut body = serde_json::Map::new();
    body.insert("ok".to_string(), Value::Bool(true));

    if let Some(obj) = payload.as_object() {
        for (key, value) in obj {
            body.insert(key.clone(), value.clone());
        }
    } else if !payload.is_null() {
        body.insert("data".to_string(), payload);
    }

    (StatusCode::OK, Json(Value::Object(body)))
}

fn ok_snapshot(snapshot: UiSnapshot) -> ApiResponse {
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "rows": snapshot.rows,
            "preview": snapshot.preview,
            "confirm_delete": snapshot.confirm_delete,
        })),
    )
}

fn err_json(status: StatusCode, message: &str) -> ApiResponse {
    (
        status,
        Json(json!({
            "ok": false,
            "error": message,
        })),
    )
}

fn build_ui_snapshot(config: &ConfigStore) -> UiSnapshot {
    let mut rows = Vec::new();
    let mut render_entries = Vec::new();

    for item in config.get_items("prompt") {
        let (mut selected, free_text) = config.get_item_state(&item.section_name, &item.key);
        if !item.choices.iter().any(|choice| choice == &selected) {
            selected = NO_SELECTION.to_string();
        }

        render_entries.push(RenderEntry {
            label: item.label.clone(),
            selected: selected.clone(),
            free_text: free_text.clone(),
        });

        rows.push(UiRow {
            item_id: item.item_id(),
            label: item.label,
            choices: item.choices,
            allow_free_text: item.allow_free_text,
            selected,
            free_text,
        });
    }

    UiSnapshot {
        rows,
        preview: render_prompt(&render_entries),
        confirm_delete: config.confirm_delete(),
    }
}

fn split_item_id(item_id: &str) -> std::result::Result<(String, String), String> {
    let Some((section, key)) = item_id.split_once(':') else {
        return Err("invalid item_id".to_string());
    };

    let section = section.trim();
    let key = key.trim();
    if section.is_empty() || key.is_empty() {
        return Err("invalid item_id".to_string());
    }

    Ok((section.to_string(), key.to_string()))
}

fn find_item(config: &ConfigStore, section: &str, key: &str) -> Option<ItemConfig> {
    config
        .get_items(section)
        .into_iter()
        .find(|item| item.key == key)
}

fn bind_listener(preferred_port: u16) -> Result<TcpListener> {
    for offset in 0..200u16 {
        let port = preferred_port.saturating_add(offset);
        if port == 0 {
            continue;
        }

        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)) {
            return Ok(listener);
        }
    }

    Err(anyhow!("failed to bind server port"))
}

#[cfg(target_os = "windows")]
fn copy_to_system_clipboard(text: &str) -> Result<()> {
    clipboard_win::set_clipboard_string(text)
        .map_err(|err| anyhow!("failed to write clipboard: {err}"))
}

#[cfg(not(target_os = "windows"))]
fn copy_to_system_clipboard(_text: &str) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn to_wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<u16>>()
}

#[cfg(target_os = "windows")]
fn open_file_in_browser(path: &Path) -> Result<()> {
    let operation = to_wide_null(std::ffi::OsStr::new("open"));
    let file = to_wide_null(path.as_os_str());

    let result = unsafe {
        windows_sys::Win32::UI::Shell::ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    let result_code = result as isize;
    if result_code <= 32 {
        return Err(anyhow!(
            "ShellExecuteW failed (code: {result_code}) for {}",
            path.display()
        ));
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn open_file_in_browser(_path: &Path) -> Result<()> {
    Ok(())
}
