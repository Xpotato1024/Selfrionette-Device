mod fixtures;
mod workflow;
use std::sync::Mutex;
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
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(Workspace::default()))
        .invoke_handler(tauri::generate_handler![device_command])
        .run(tauri::generate_context!())
        .expect("Device Manager startup failed");
}
