//! Native adapter only: explicit port ownership and local application storage.
//! Fitting, capture, protocol management and runtime belong to the shared service.
use crate::discovery::{PortKey, PortView};
use selfrionette_serial::app::{Command, LiveService};
use selfrionette_serial::{SerialPortTransport, available_port_summaries};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Default)]
pub struct Application {
    service: Option<LiveService>,
    target: Option<PortKey>,
    fixture: Option<crate::scripted::Shared>,
}
impl Application {
    pub fn scripted() -> Self {
        Self {
            fixture: Some(std::sync::Arc::new(std::sync::Mutex::new(
                crate::scripted::State::default(),
            ))),
            ..Self::default()
        }
    }
    pub fn connected(&self) -> bool {
        self.service.is_some()
    }
    pub fn execute(&mut self, mut command: Value, root: &Path) -> Result<Value, String> {
        let action = command
            .get("action")
            .and_then(Value::as_str)
            .ok_or("操作がありません。")?
            .to_owned();
        if action == "fixture_input" {
            let fixture = self
                .fixture
                .as_ref()
                .ok_or("scripted hardwareは明示的な開発者起動だけで利用できます。")?;
            let mut state = fixture
                .lock()
                .map_err(|_| "fixture状態を取得できません。")?;
            if let Some(raw) = command.get("raw") {
                let next: [i32; 7] =
                    serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
                if next.iter().any(|v| !(-8388608..=8388607).contains(v)) {
                    return Err("fixture raw range".into());
                }
                state.raw = next;
            }
            if let Some(flags) = command.get("flags") {
                state.flags = serde_json::from_value(flags.clone()).map_err(|e| e.to_string())?;
            }
            if let Some(connected) = command.get("connected").and_then(Value::as_bool) {
                state.connected = connected;
            }
            return Ok(json!({"scripted":true}));
        }
        if action == "ports" {
            if self.fixture.is_some() {
                return Ok(
                    json!({"ports":[{"name":"SCRIPTED","kind":"usb","serial_number":"developer-only","product":"Scripted transport · 実機ではありません","manufacturer":"developer fixture","vid":6991,"pid":37382,"category":"candidate"}]}),
                );
            }
            let ports = available_port_summaries()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(PortView::from)
                .collect::<Vec<_>>();
            return Ok(json!({"ports": ports}));
        }
        if action == "disconnect" {
            if let Some(service) = self.service.as_mut() {
                let disconnect: Command = serde_json::from_value(json!({"action":"disconnect"}))
                    .map_err(|e| e.to_string())?;
                // Checkpoint measured evidence before releasing the handle.
                if let Err(error) = service.execute(disconnect, root) {
                    return Err(error.to_string());
                }
            }
            // Dropping the exclusively owned transport releases it even after failure.
            self.service = None;
            self.target = None;
            return Ok(json!({"view":{"connected":false,"ready":false}}));
        }
        if action == "snapshot" && self.service.is_none() {
            return Ok(json!({"view":{"connected":false,"ready":false}}));
        }
        if action == "connect" {
            if self.connected() {
                return Err("先に現在の品質付き接続を切断してください。".into());
            }
            let target: PortKey =
                serde_json::from_value(command.get("target").cloned().ok_or("対象がありません。")?)
                    .map_err(|e| e.to_string())?;
            let transport: Box<dyn selfrionette_serial::LineTransport + Send> =
                if let Some(fixture) = &self.fixture {
                    if target.name != "SCRIPTED" {
                        return Err("scripted起動ではOS portを開きません。".into());
                    }
                    Box::new(crate::scripted::ScriptedTransport::new(fixture.clone()))
                } else {
                    let ports = available_port_summaries()
                        .map_err(|e| e.to_string())?
                        .into_iter()
                        .map(PortView::from)
                        .collect::<Vec<_>>();
                    if !ports
                        .iter()
                        .any(|p| p.target == target && p.category != "bootloader")
                    {
                        return Err(
                            "接続先のUSB情報が変わりました。再検索して確認してください。".into(),
                        );
                    }
                    Box::new(
                        SerialPortTransport::open_with_dtr(&target.name)
                            .map_err(|e| e.to_string())?,
                    )
                };
            let mut service = LiveService::new(transport);
            let parsed: Command =
                serde_json::from_value(json!({"action":"connect"})).map_err(|e| e.to_string())?;
            let reply = service.execute(parsed, root).map_err(|e| e.to_string())?;
            self.service = Some(service);
            self.target = Some(target);
            return self.reply(reply);
        }
        if action == "provision" {
            if command.get("confirmed").and_then(Value::as_bool) != Some(true) {
                return Err("未登録の対象を明示確認してください。".into());
            }
            command
                .as_object_mut()
                .ok_or("操作形式が不正です。")?
                .insert("device_id".into(), Value::String(random_device_id()?));
        }
        let parsed: Command = serde_json::from_value(command).map_err(|e| e.to_string())?;
        let service = self
            .service
            .as_mut()
            .ok_or("品質付きFirmwareに接続してください。")?;
        match service.execute(parsed, root) {
            Ok(reply) => self.reply(reply),
            Err(error) => {
                let message = error.to_string();
                let snapshot: Command = serde_json::from_value(json!({"action":"snapshot"}))
                    .map_err(|e| e.to_string())?;
                match service.execute(snapshot, root) {
                    Ok(reply) => {
                        let mut result = self.reply(reply)?;
                        result["error"] = Value::String(message);
                        Ok(result)
                    }
                    Err(_) => {
                        self.service = None;
                        self.target = None;
                        Ok(json!({"view":{"connected":false,"ready":false},"error":message}))
                    }
                }
            }
        }
    }
    fn reply(&self, reply: impl serde::Serialize) -> Result<Value, String> {
        let value = serde_json::to_value(reply).map_err(|e| e.to_string())?;
        let mut result = if value.get("view").is_some() {
            value
        } else {
            json!({"view":value})
        };
        if result["view"].get("connected").is_none() {
            result["view"]["connected"] = Value::Bool(self.connected());
        }
        result["view"]["owns_handle"] = Value::Bool(self.connected());
        result["view"]["scripted"] = Value::Bool(self.fixture.is_some());
        result["view"]["port"] = json!(self.target.as_ref().map(|t| &t.name));
        Ok(result)
    }
}

fn random_device_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(
                algorithm: *mut std::ffi::c_void,
                buffer: *mut u8,
                length: u32,
                flags: u32,
            ) -> i32;
        }
        // BCRYPT_USE_SYSTEM_PREFERRED_RNG. No persistent provider or credential.
        if unsafe { BCryptGenRandom(std::ptr::null_mut(), bytes.as_mut_ptr(), 16, 2) } < 0 {
            return Err("OS乱数生成に失敗しました。IDは登録していません。".into());
        }
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes))
            .map_err(|e| e.to_string())?;
    }
    if bytes == [0; 16] {
        return Err("OS乱数生成結果が無効です。".into());
    }
    Ok(format!(
        "srn-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    struct Store(std::path::PathBuf);
    impl Store {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "selfrionette-native-adapter-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Store {
        fn drop(&mut self) {
            assert!(self.0.is_absolute());
            assert!(self.0.starts_with(std::env::temp_dir()));
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn connect(app: &mut Application, store: &Path) -> Value {
        app.execute(json!({"action":"connect","target":{"name":"SCRIPTED","kind":"usb","serial_number":"developer-only","product":"Scripted transport","manufacturer":"fixture","vid":6991,"pid":37382}}),store).unwrap()
    }
    #[test]
    fn ordinary_adapter_rejects_fixture_input_without_creating_storage() {
        let mut app = Application::default();
        let absent = std::env::temp_dir().join("must-not-create-selfrionette-fixture-store");
        assert!(
            app.execute(
                json!({"action":"fixture_input","raw":[1,2,3,4,5,6,7]}),
                &absent
            )
            .is_err()
        );
        assert!(!app.connected());
    }
    #[test]
    fn scripted_transport_uses_native_service_provision_readback_and_rejects_rewrite() {
        let store = Store::new();
        let mut app = Application::scripted();
        let initial = connect(&mut app, &store.0);
        assert_eq!(initial["view"]["info"]["device_id"], "unprovisioned");
        let reply = app
            .execute(json!({"action":"provision","confirmed":true}), &store.0)
            .unwrap();
        let id = reply["view"]["info"]["device_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(id.parse::<selfrionette_core::DeviceId>().is_ok());
        let second = app
            .execute(json!({"action":"provision","confirmed":true}), &store.0)
            .unwrap();
        assert!(second["error"].is_string());
        assert_eq!(second["view"]["info"]["device_id"], id);
        app.execute(json!({"action":"disconnect"}), &store.0)
            .unwrap();
        assert!(!app.connected());
    }
    #[test]
    fn invalid_scripted_raw_does_not_mutate_the_test_sensor_state() {
        let store = Store::new();
        let mut app = Application::scripted();
        assert!(
            app.execute(
                json!({"action":"fixture_input","raw":[8388608,0,0,0,0,0,0]}),
                &store.0
            )
            .is_err()
        );
        assert_eq!(
            app.fixture.as_ref().unwrap().lock().unwrap().raw,
            [1_000_000; 7]
        );
    }
}
