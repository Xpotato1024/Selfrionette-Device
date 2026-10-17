mod application;
mod discovery;
mod firmware_family;
mod fixtures;
mod hardware;
mod preferences;
mod scripted;
mod session;
mod updater;
mod workflow;
use session::{Mode, ModeCommand, ModeRequest, Runtime, Source};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
type Shared = Arc<Mutex<Runtime>>;

#[tauri::command]
async fn connection_candidate(
    app: tauri::AppHandle,
    target: Option<discovery::PortKey>,
    state: tauri::State<'_, UpdateShared>,
) -> Result<Option<discovery::PortKey>, String> {
    let root = match std::env::var_os("SELFRIONETTE_GUI_FIXTURE_DIR") {
        Some(p) => std::path::PathBuf::from(p),
        None => app.path().app_config_dir().map_err(|e| e.to_string())?,
    };
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(target) = target {
            if let Some(s) = state
                .try_lock()
                .map_err(|_| "更新処理中です。操作は予約されません。")?
                .as_mut()
            {
                s.invalidate()?;
            }
            preferences::save(&root, target)?;
        }
        preferences::load(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn updater_invalidate(state: tauri::State<'_, UpdateShared>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = state
            .try_lock()
            .map_err(|_| "更新処理中です。操作は予約されません。")?;
        if let Some(s) = s.as_mut() {
            s.invalidate()?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn application_command(
    app: tauri::AppHandle,
    command: serde_json::Value,
    state: tauri::State<'_, Shared>,
    updater: tauri::State<'_, UpdateShared>,
) -> Result<serde_json::Value, String> {
    let menu_changed = matches!(
        command.get("action").and_then(serde_json::Value::as_str),
        Some("connect" | "disconnect" | "pause" | "reconnect")
    );
    let state = state.inner().clone();
    let updater = updater.inner().clone();
    let passive = matches!(
        command.get("action").and_then(serde_json::Value::as_str),
        Some("snapshot" | "ports" | "stream_tick" | "capture_tick" | "tick")
    );
    let expected_epoch = if passive {
        None
    } else {
        Some(
            state
                .try_lock()
                .map_err(|_| "別の処理中です。操作は予約されません。")?
                .operation_epoch,
        )
    };
    let root = match std::env::var_os("SELFRIONETTE_GUI_FIXTURE_DIR") {
        Some(root) => std::path::PathBuf::from(root).join("personal-profiles"),
        None => app
            .path()
            .app_config_dir()
            .map_err(|e| e.to_string())?
            .join("personal-profiles"),
    };
    let (reply, mode, connected) = tauri::async_runtime::spawn_blocking(move || {
        let passive = matches!(
            command.get("action").and_then(serde_json::Value::as_str),
            Some("snapshot" | "ports" | "stream_tick" | "capture_tick" | "tick")
        );
        let mut runtime = if passive {
            state.lock().map_err(|_| "アプリ状態を取得できません。")?
        } else {
            state
                .try_lock()
                .map_err(|_| "別の処理中です。操作は予約されません。")?
        };
        if let Some(epoch) = expected_epoch {
            runtime.advance_operation(epoch)?;
        }
        if !passive
            && let Some(s) = updater
                .lock()
                .map_err(|_| "更新状態を取得できません。")?
                .as_mut()
        {
            s.invalidate()?;
        }
        if runtime.mode.source != Source::Hardware {
            return Err("実機設定は実機画面で実行してください。".into());
        }
        if runtime.monitor.connected()
            && matches!(
                command.get("action").and_then(serde_json::Value::as_str),
                Some("connect" | "reconnect")
            )
        {
            return Err("入力確認の接続を先に切断してください。".into());
        }
        let root = if runtime.mode.scripted_hardware {
            root.join("scripted")
        } else {
            root
        };
        let reply = runtime.application.execute(command, &root)?;
        Ok::<_, String>((
            reply,
            runtime.mode.clone(),
            runtime.monitor.connected() || runtime.application.connected(),
        ))
    })
    .await
    .map_err(|e| e.to_string())??;
    if menu_changed {
        install_menu(&app, &mode, connected).map_err(|e| e.to_string())?;
    }
    Ok(reply)
}

type UpdateShared = Arc<Mutex<Option<updater::UpdaterService>>>;
fn updater_service<'a>(
    app: &tauri::AppHandle,
    state: &'a mut Option<updater::UpdaterService>,
) -> Result<&'a mut updater::UpdaterService, String> {
    if state.is_none() {
        let resources = app.path().resource_dir().map_err(|e| e.to_string())?;
        let config = match std::env::var_os("SELFRIONETTE_GUI_FIXTURE_DIR") {
            Some(p) => std::path::PathBuf::from(p).join("updater"),
            None => app
                .path()
                .app_config_dir()
                .map_err(|e| e.to_string())?
                .join("updater"),
        };
        *state = Some(updater::UpdaterService::new(resources, config));
    }
    state
        .as_mut()
        .ok_or_else(|| "更新状態を取得できません。".into())
}
#[tauri::command]
async fn updater_prepare(
    app: tauri::AppHandle,
    request: updater::PrepareRequest,
    state: tauri::State<'_, UpdateShared>,
    runtime: tauri::State<'_, Shared>,
) -> Result<updater::PrepareReply, String> {
    let state = state.inner().clone();
    let runtime = runtime.inner().clone();
    let expected_epoch = runtime
        .try_lock()
        .map_err(|_| "別の処理中です。操作は予約されません。")?
        .operation_epoch;
    tauri::async_runtime::spawn_blocking(move || {
        let mut r = runtime
            .try_lock()
            .map_err(|_| "別の処理中です。完了後に操作してください。操作は予約されません。")?;
        r.require_unowned()?;
        r.advance_operation(expected_epoch)?;
        let mut s = state.lock().map_err(|_| "更新状態を取得できません。")?;
        updater_service(&app, &mut s)?.prepare(request)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn updater_apply(
    app: tauri::AppHandle,
    request: updater::ApplyRequest,
    state: tauri::State<'_, UpdateShared>,
    runtime: tauri::State<'_, Shared>,
) -> Result<updater::ApplyReply, String> {
    let runtime = runtime.inner().clone();
    let state = state.inner().clone();
    let expected_epoch = runtime
        .try_lock()
        .map_err(|_| "別の処理中です。操作は予約されません。")?
        .operation_epoch;
    tauri::async_runtime::spawn_blocking(move || {
        let mut r = runtime
            .try_lock()
            .map_err(|_| "別の処理中です。完了後に操作してください。操作は予約されません。")?;
        r.require_unowned()?;
        r.advance_operation(expected_epoch)?;
        let mut s = state.lock().map_err(|_| "更新状態を取得できません。")?;
        updater_service(&app, &mut s)?.apply(request)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn updater_recover(
    app: tauri::AppHandle,
    metadata_supported: Option<bool>,
    state: tauri::State<'_, UpdateShared>,
    runtime: tauri::State<'_, Shared>,
) -> Result<updater::RecoveryReply, String> {
    let runtime = runtime.inner().clone();
    let state = state.inner().clone();
    let expected_epoch = runtime
        .try_lock()
        .map_err(|_| "別の処理中です。操作は予約されません。")?
        .operation_epoch;
    tauri::async_runtime::spawn_blocking(move || {
        let mut r = runtime
            .try_lock()
            .map_err(|_| "別の処理中です。完了後に操作してください。操作は予約されません。")?;
        r.require_unowned()?;
        r.advance_operation(expected_epoch)?;
        let mut s = state.lock().map_err(|_| "更新状態を取得できません。")?;
        updater_service(&app, &mut s)?.recover(metadata_supported.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn device_command(
    app: tauri::AppHandle,
    request: workflow::Request,
    state: tauri::State<'_, Shared>,
) -> Result<workflow::Reply, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state.lock().map_err(|_| "アプリ状態を取得できません。")?;
        runtime.mode.require_synthetic()?;
        let root = match std::env::var_os("SELFRIONETTE_GUI_FIXTURE_DIR") {
            Some(root) => root.into(),
            None => app
                .path()
                .app_config_dir()
                .map_err(|e| e.to_string())?
                .join("synthetic-fixtures"),
        };
        runtime.synthetic(request, &root)
    })
    .await
    .map_err(|e| e.to_string())?
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
    app: tauri::AppHandle,
    request: hardware::Request,
    state: tauri::State<'_, Shared>,
    updater: tauri::State<'_, UpdateShared>,
) -> Result<hardware::Reply, String> {
    let state = state.inner().clone();
    let updater = updater.inner().clone();
    let passive = matches!(
        request.command,
        hardware::Command::Snapshot {} | hardware::Command::Read {} | hardware::Command::Scan {}
    );
    let expected_epoch = if passive {
        None
    } else {
        Some(
            state
                .try_lock()
                .map_err(|_| "別の処理中です。操作は予約されません。")?
                .operation_epoch,
        )
    };
    let (reply, mode, changed, connected) = tauri::async_runtime::spawn_blocking(move || {
        let passive = matches!(
            request.command,
            hardware::Command::Snapshot {}
                | hardware::Command::Read {}
                | hardware::Command::Scan {}
        );
        let mut runtime = if passive {
            state.lock().map_err(|_| "接続状態を取得できません。")?
        } else {
            state
                .try_lock()
                .map_err(|_| "別の処理中です。操作は予約されません。")?
        };
        if let Some(epoch) = expected_epoch {
            runtime.advance_operation(epoch)?;
        }
        let before = runtime.monitor.connected() || runtime.application.connected();
        if !passive
            && let Some(s) = updater
                .lock()
                .map_err(|_| "更新状態を取得できません。")?
                .as_mut()
        {
            s.invalidate()?;
        }
        let reply = runtime.hardware(request)?;
        let connected = runtime.monitor.connected() || runtime.application.connected();
        Ok::<_, String>((reply, runtime.mode.clone(), before != connected, connected))
    })
    .await
    .map_err(|e| e.to_string())??;
    if changed {
        install_menu(&app, &mode, connected).map_err(|e| e.to_string())?;
    }
    Ok(reply)
}
#[tauri::command]
async fn mode_command(
    app: tauri::AppHandle,
    request: ModeRequest,
    state: tauri::State<'_, Shared>,
) -> Result<Mode, String> {
    let change = !matches!(request.command, ModeCommand::Snapshot);
    let state = state.inner().clone();
    let (mode, connected) = tauri::async_runtime::spawn_blocking(move || {
        let mut runtime = state
            .try_lock()
            .map_err(|_| "アプリ状態を取得できません。".to_string())?;
        let mode = runtime.set_mode(request)?;
        Ok::<_, String>((
            mode,
            runtime.monitor.connected() || runtime.application.connected(),
        ))
    })
    .await
    .map_err(|e| e.to_string())??;
    if change {
        install_menu(&app, &mode, connected).map_err(|e| e.to_string())?;
        app.emit_to("main", "manager-mode", &mode)
            .map_err(|e| e.to_string())?;
    }
    Ok(mode)
}
fn install_menu(app: &tauri::AppHandle, mode: &Mode, connected: bool) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItem, SubmenuBuilder};
    let choose = MenuItem::with_id(
        app,
        "choose-device",
        "接続先を選ぶ…",
        mode.source == Source::Hardware && !connected,
        Some("Ctrl+O"),
    )?;
    let device = SubmenuBuilder::new(app, "デバイス(&D)")
        .item(&choose)
        .item(&MenuItem::with_id(
            app,
            "disconnect",
            "切断",
            connected,
            None::<&str>,
        )?)
        .text("pause", "一時停止（接続を解放）")
        .text("reconnect", "再開・再接続…")
        .separator()
        .text("quit", "終了")
        .build()?;
    let input = MenuItem::with_id(app, "view-device", "入力確認", true, Some("Ctrl+1"))?;
    let adjust = MenuItem::with_id(app, "view-adjust", "調整", true, Some("Ctrl+2"))?;
    let diagnostics =
        MenuItem::with_id(app, "view-diagnostics", "診断・記録", true, Some("Ctrl+3"))?;
    let view = SubmenuBuilder::new(app, "表示(&V)")
        .items(&[&input, &adjust, &diagnostics])
        .build()?;
    let mut help = SubmenuBuilder::new(app, "ヘルプ(&H)")
        .text("help", "使い方")
        .text("about", "アプリ情報");
    if !mode.developer {
        help = help
            .separator()
            .text("enable-developer", "開発者向け機能を有効にする…");
    }
    let help = help.build()?;
    let mut menu = MenuBuilder::new(app).items(&[&device, &view]);
    if mode.developer {
        let real = MenuItem::with_id(
            app,
            "source-hardware",
            "実機画面",
            mode.source != Source::Hardware,
            None::<&str>,
        )?;
        let synthetic = MenuItem::with_id(
            app,
            "source-synthetic",
            "合成試験",
            !connected && mode.source != Source::Synthetic,
            None::<&str>,
        )?;
        let developer = SubmenuBuilder::new(app, "開発(&T)")
            .items(&[&real, &synthetic])
            .separator()
            .text("disable-developer", "開発者モードを終了")
            .build()?;
        menu = menu.item(&developer);
    }
    app.set_menu(menu.item(&help).build()?)?;
    Ok(())
}
pub fn run() {
    let mode = match Mode::startup(std::env::args().skip(1)) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let startup = mode.clone();
    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(Runtime::new(mode))))
        .manage(Arc::new(Mutex::new(None::<updater::UpdaterService>)))
        .setup(move |app| {
            install_menu(app.handle(), &startup, false)?;
            app.on_menu_event(|app, event| {
                if event.id().as_ref() == "quit" {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.close();
                    }
                } else if let Err(error) = app.emit_to("main", "manager-menu", event.id().as_ref())
                {
                    eprintln!("menu: {error}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            device_command,
            application_info,
            hardware_command,
            application_command,
            updater_prepare,
            updater_apply,
            updater_recover,
            updater_invalidate,
            connection_candidate,
            mode_command
        ])
        .run(tauri::generate_context!())
        .expect("Device Manager startup failed");
}

#[cfg(test)]
mod frontend_embedding;
