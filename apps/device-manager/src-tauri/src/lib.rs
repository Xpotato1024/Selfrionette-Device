mod fixtures;
mod hardware;
mod workflow;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use workflow::{Reply, Request, Workspace};

#[tauri::command]
fn device_command(
    app: tauri::AppHandle,
    request: Request,
    state: tauri::State<Mutex<Workspace>>,
) -> Result<Reply, String> {
    let root = match std::env::var_os("SELFRIONETTE_GUI_FIXTURE_DIR") {
        Some(root) => root.into(),
        None => app
            .path()
            .app_config_dir()
            .map_err(|e| e.to_string())?
            .join("synthetic-fixtures"),
    };
    Ok(state
        .lock()
        .map_err(|_| "state unavailable")?
        .execute(request, &root))
}
#[derive(serde::Serialize)]
struct ApplicationInfo {
    product_name: String,
    version: String,
    publisher: Option<String>,
    homepage: Option<String>,
}

#[tauri::command]
fn application_info(app: tauri::AppHandle) -> Result<ApplicationInfo, String> {
    // Tauri strips publisher/homepage from its generated runtime Config.
    // Embed the source-of-truth file; never require the user's clone at runtime.
    let metadata: tauri::Config = serde_json::from_str(include_str!("../tauri.conf.json"))
        .map_err(|error| error.to_string())?;
    let config = app.config();
    Ok(ApplicationInfo {
        product_name: config.product_name.clone().unwrap_or_default(),
        version: app.package_info().version.to_string(),
        publisher: metadata.bundle.publisher,
        homepage: metadata.bundle.homepage,
    })
}

#[tauri::command]
async fn hardware_command(
    request: hardware::Request,
    state: tauri::State<'_, Arc<Mutex<hardware::Monitor>>>,
) -> Result<hardware::Reply, String> {
    let monitor = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        monitor
            .lock()
            .map_err(|_| "接続状態を取得できません。".to_string())
            .map(|mut m| m.execute(request))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(Workspace::default()))
        .manage(Arc::new(Mutex::new(hardware::Monitor::default())))
        .invoke_handler(tauri::generate_handler![
            device_command,
            application_info,
            hardware_command
        ])
        .run(tauri::generate_context!())
        .expect("Device Manager startup failed");
}
