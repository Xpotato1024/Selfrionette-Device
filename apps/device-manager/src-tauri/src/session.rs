//! Process-scoped feature gate. This does not grant hardware or firmware-write permission.
use crate::{application, hardware, workflow};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Hardware,
    Synthetic,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Mode {
    pub developer: bool,
    pub source: Source,
    pub epoch: u64,
    pub scripted_hardware: bool,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModeCommand {
    Snapshot,
    Enable,
    Disable,
    Select { source: Source },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeRequest {
    pub expected: Option<u64>,
    pub command: ModeCommand,
}
impl Mode {
    pub fn startup(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut developer = false;
        let mut synthetic = false;
        let mut scripted_hardware = false;
        for arg in args {
            match arg.as_str() {
                "--developer-mode" if !developer => developer = true,
                "--synthetic" if !synthetic => synthetic = true,
                "--scripted-hardware" if !scripted_hardware => scripted_hardware = true,
                _ => {
                    return Err(format!(
                        "未対応または重複した引数: {arg}\n使用可能: --developer-mode [--synthetic | --scripted-hardware]"
                    ));
                }
            }
        }
        if synthetic && !developer {
            return Err("--synthetic には --developer-mode が必要です。".into());
        }
        if scripted_hardware && (!developer || synthetic) {
            return Err("--scripted-hardwareには--developer-modeと実機画面が必要です。".into());
        }
        Ok(Self {
            developer,
            source: if synthetic {
                Source::Synthetic
            } else {
                Source::Hardware
            },
            epoch: 0,
            scripted_hardware,
        })
    }
    pub fn apply(&mut self, request: ModeRequest, connected: bool) -> Result<Self, String> {
        if matches!(request.command, ModeCommand::Snapshot) {
            return Ok(self.clone());
        }
        if request.expected != Some(self.epoch) {
            return Err("表示モードが変わりました。操作をやり直してください。".into());
        }
        let mut next = self.clone();
        match request.command {
            ModeCommand::Snapshot => unreachable!(),
            ModeCommand::Enable => next.developer = true,
            ModeCommand::Disable => {
                if self.scripted_hardware {
                    return Err(
                        "scripted hardwareを終了するにはアプリを閉じて通常起動してください。"
                            .into(),
                    );
                }
                next.developer = false;
                next.source = Source::Hardware;
            }
            ModeCommand::Select { source } => {
                if source == Source::Synthetic && !self.developer {
                    return Err("開発者向け機能が無効です。".into());
                }
                if source != self.source && connected {
                    return Err("先に実機との接続を切断してください。".into());
                }
                next.source = source;
            }
        }
        next.epoch = self
            .epoch
            .checked_add(1)
            .ok_or("モード更新回数の上限です。再起動してください。")?;
        *self = next;
        Ok(self.clone())
    }
    pub fn require_synthetic(&self) -> Result<(), String> {
        if self.developer && self.source == Source::Synthetic {
            Ok(())
        } else {
            Err("合成試験は開発者モードの合成画面でのみ実行できます。".into())
        }
    }
}
pub struct Runtime {
    pub mode: Mode,
    pub workspace: workflow::Workspace,
    pub monitor: hardware::Monitor,
    pub application: application::Application,
}
impl Runtime {
    pub fn new(mode: Mode) -> Self {
        let application = if mode.scripted_hardware {
            application::Application::scripted()
        } else {
            application::Application::default()
        };
        Self {
            mode,
            workspace: workflow::Workspace::default(),
            monitor: hardware::Monitor::default(),
            application,
        }
    }
    pub fn synthetic(
        &mut self,
        request: workflow::Request,
        root: &Path,
    ) -> Result<workflow::Reply, String> {
        self.mode.require_synthetic()?;
        Ok(self.workspace.execute(request, root))
    }
    pub fn hardware(&mut self, request: hardware::Request) -> Result<hardware::Reply, String> {
        if self.application.connected()
            && matches!(request.command, hardware::Command::Connect { .. })
        {
            return Err("先に調整画面の品質付き接続を解放してください。".into());
        }
        if self.mode.source != Source::Hardware
            && !matches!(request.command, hardware::Command::Snapshot {})
        {
            return Err("合成画面では実機通信を実行できません。".into());
        }
        Ok(self.monitor.execute(request))
    }
    pub fn set_mode(&mut self, request: ModeRequest) -> Result<Mode, String> {
        self.mode.apply(
            request,
            self.monitor.connected() || self.application.connected(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Result<Mode, String> {
        Mode::startup(values.iter().map(|s| s.to_string()))
    }
    fn change(m: &mut Mode, c: ModeCommand, connected: bool) -> Result<Mode, String> {
        m.apply(
            ModeRequest {
                expected: Some(m.epoch),
                command: c,
            },
            connected,
        )
    }
    #[test]
    fn ordinary_startup_is_closed_even_for_debug_or_fixture_directories() {
        let m = args(&[]).unwrap();
        assert!(!m.developer);
        assert_eq!(m.source, Source::Hardware);
        assert!(m.require_synthetic().is_err());
        assert!(args(&["--synthetic"]).is_err());
        assert!(args(&["--developer-mode", "--unknown"]).is_err());
        assert!(args(&["--developer-mode", "--developer-mode"]).is_err());
        assert!(args(&["--scripted-hardware"]).is_err());
        assert!(args(&["--developer-mode", "--synthetic", "--scripted-hardware"]).is_err());
        assert!(
            args(&["--developer-mode", "--scripted-hardware"])
                .unwrap()
                .scripted_hardware
        );
    }
    #[test]
    fn explicit_cli_and_gui_modes_have_identical_gates_and_no_persistence() {
        assert!(
            args(&["--developer-mode"])
                .unwrap()
                .require_synthetic()
                .is_err()
        );
        assert!(
            args(&["--developer-mode", "--synthetic"])
                .unwrap()
                .require_synthetic()
                .is_ok()
        );
        let mut m = Mode::default();
        change(&mut m, ModeCommand::Enable, false).unwrap();
        assert_eq!(m.source, Source::Hardware);
        change(
            &mut m,
            ModeCommand::Select {
                source: Source::Synthetic,
            },
            false,
        )
        .unwrap();
        assert!(m.require_synthetic().is_ok());
        change(&mut m, ModeCommand::Disable, false).unwrap();
        assert!(m.require_synthetic().is_err());
        assert!(!Mode::default().developer);
    }
    #[test]
    fn connection_and_stale_mode_requests_cannot_be_bypassed() {
        let mut m = args(&["--developer-mode"]).unwrap();
        let before = m.clone();
        assert!(
            change(
                &mut m,
                ModeCommand::Select {
                    source: Source::Synthetic
                },
                true
            )
            .is_err()
        );
        assert_eq!(m, before);
        assert!(
            m.apply(
                ModeRequest {
                    expected: Some(99),
                    command: ModeCommand::Enable
                },
                false
            )
            .is_err()
        );
    }
    #[test]
    fn actual_ipc_use_cases_reject_before_synthetic_files_or_port_enumeration() {
        let mut r = Runtime::new(Mode::default());
        let request =
            serde_json::from_str(r#"{"expected":null,"command":{"action":"status"}}"#).unwrap();
        assert!(
            r.synthetic(request, Path::new("MUST_NOT_BE_CREATED"))
                .is_err()
        );
        r.mode = args(&["--developer-mode", "--synthetic"]).unwrap();
        assert!(
            r.hardware(hardware::Request {
                expected: None,
                command: hardware::Command::Scan {}
            })
            .is_err()
        );
    }
}
