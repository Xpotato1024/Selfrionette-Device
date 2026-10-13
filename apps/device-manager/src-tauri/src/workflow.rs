//! Synthetic device use cases. No Tauri, serial, or robot operations belong here.
use crate::fixtures::{draft_json, trials};
use selfrionette_core::calibration::*;
use selfrionette_core::persistence::{ProfileStore, resume};
use selfrionette_core::{DeviceId, VectorFrame};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Left,
    Right,
}
impl Role {
    fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
    fn id(self) -> DeviceId {
        match self {
            Self::Left => "srn-11111111111111111111111111111111",
            Self::Right => "srn-22222222222222222222222222222222",
        }
        .parse()
        .expect("fixed synthetic identity")
    }
    fn context(self, epoch: u64) -> Context {
        Context {
            source: Source {
                id: self.id(),
                firmware: "synthetic-v2".into(),
                assembly: "fixture-assembly-1".into(),
                epoch,
            },
            user: "fixture-user".into(),
            posture: "fixture-neutral".into(),
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub epoch: u64,
    pub revision: u64,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Status {},
    Select { role: Role },
    Begin {},
    Baseline {},
    Assess { scenario: String },
    Flip { channel: usize, reason: String },
    Preview { level: String },
    Confirm {},
    Save {},
    Export {},
    ReadHistory {},
    Resume {},
    Invalidate {},
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected: Option<Target>,
    pub command: Command,
}
#[derive(Default)]
struct DeviceState {
    session: Option<CalibrationSession>,
    epoch: u64,
    history: Option<Draft>,
    saved: Option<Preview>,
}
#[derive(Default)]
pub struct Workspace {
    selected: Role,
    devices: [DeviceState; 2],
    revision: u64,
}
#[derive(Debug, Serialize)]
pub struct Operation {
    pub id: &'static str,
    pub enabled: bool,
    pub reason: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Step {
    pub label: &'static str,
    pub state: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Row {
    pub wire: usize,
    pub physical: Option<usize>,
    pub firmware: f64,
    pub baseline: f64,
    pub delta: f64,
    pub sign: i8,
    pub oriented: f64,
    pub positive_span: f64,
    pub normalized: Option<f64>,
    pub displayed: Option<f64>,
    pub saturated: bool,
    pub evidence: String,
}
#[derive(Debug, Serialize)]
pub struct DeviceSummary {
    pub role: Role,
    pub id: String,
    pub phase: &'static str,
}
#[derive(Debug, Serialize)]
pub struct View {
    pub token: Target,
    pub id: String,
    pub role: Role,
    pub epoch: u64,
    pub core_revision: Option<u64>,
    pub phase: &'static str,
    pub guidance: &'static str,
    pub next_action: Option<&'static str>,
    pub operations: Vec<Operation>,
    pub steps: Vec<Step>,
    pub devices: Vec<DeviceSummary>,
    pub rows: Vec<Row>,
    pub history: Option<serde_json::Value>,
    pub saved_current: bool,
    pub hardware_verified: bool,
    pub control_ready: bool,
}
#[derive(Debug, Serialize)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub detail: String,
    pub field: Option<&'static str>,
}
impl Failure {
    fn new(code: &'static str, message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: detail.into(),
            field: None,
        }
    }
    fn condition(detail: impl Into<String>) -> Self {
        Self::new(
            "precondition",
            "必要な条件が未完了です。案内された工程から確認してください。",
            detail,
        )
    }
}
#[derive(Debug, Serialize)]
pub struct Reply {
    pub view: View,
    pub error: Option<Failure>,
    pub export: Option<String>,
    pub message: String,
}
impl DeviceState {
    fn phase(&self) -> &'static str {
        use CalibrationPhase::*;
        match self.session.as_ref().map(CalibrationSession::phase) {
            None => "not_started",
            Some(NeedsBaseline) => "baseline",
            Some(NeedsAssessment) => "assessment",
            Some(NeedsPreview) => "preview",
            Some(NeedsConfirmation) => "confirmation",
            Some(Confirmed) => "record",
        }
    }
    fn saved_current(&self) -> bool {
        self.session.as_ref().is_some_and(|s| {
            s.phase() == CalibrationPhase::Confirmed
                && self.saved.is_some()
                && s.current_preview() == self.saved.as_ref()
        })
    }
    fn session(&mut self) -> Result<&mut CalibrationSession, Failure> {
        self.session
            .as_mut()
            .ok_or_else(|| Failure::condition("start fixture first"))
    }
    fn revoke_preview(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.revoke_preview();
        }
    }
}
impl Workspace {
    fn current(&self) -> &DeviceState {
        &self.devices[self.selected.index()]
    }
    fn target(&self) -> Target {
        Target {
            id: self.selected.id().to_string(),
            epoch: self.current().epoch,
            revision: self.revision,
        }
    }
    pub fn view(&self) -> View {
        let d = self.current();
        let phase = d.phase();
        let p = d
            .session
            .as_ref()
            .and_then(CalibrationSession::current_preview);
        let saturated = p.is_some_and(|p| p.saturated.iter().any(|v| *v));
        let started = d.session.is_some();
        let baseline = started && phase != "baseline";
        let candidates = matches!(phase, "preview" | "confirmation" | "record");
        let confirmed = phase == "record";
        let saved_current = d.saved_current();
        let operations = vec![
            ("begin", true, ""),
            (
                "baseline",
                started,
                "この個体の合成調整を開始してください。",
            ),
            ("assess", baseline, "先に今回の基準を確認してください。"),
            ("flip", candidates, "品質を満たす応答候補が必要です。"),
            (
                "preview",
                candidates,
                "先に応答と操作範囲を確認してください。",
            ),
            (
                "confirm",
                phase == "confirmation" && !saturated,
                if saturated {
                    "表示範囲を超えています。入力を変えて比較し直してください。"
                } else {
                    "未確認のプレビューが必要です。"
                },
            ),
            (
                "save",
                confirmed && !saved_current,
                if saved_current {
                    "この結果は保存済みです。"
                } else {
                    "先に現在のプレビューを確認してください。"
                },
            ),
            (
                "export",
                confirmed,
                "先に現在のプレビューを確認してください。",
            ),
            ("read_history", true, ""),
            (
                "resume",
                d.history.is_some(),
                "先にこの個体の保存記録を表示してください。",
            ),
            (
                "invalidate",
                started,
                "この個体の合成調整を開始してください。",
            ),
        ]
        .into_iter()
        .map(|(id, enabled, reason)| Operation {
            id,
            enabled,
            reason: if enabled { "" } else { reason },
        })
        .collect();
        let (next_action, guidance) = match phase {
            "not_started" => (
                Some("begin"),
                "対象個体を確認して、合成入力の調整を開始してください。",
            ),
            "baseline" => (
                Some("baseline"),
                "今回の合成入力に対する基準を確認してください。",
            ),
            "assessment" => (
                Some("assess"),
                "既知方向・反復・除荷の合成観測から、応答と操作範囲を確認してください。",
            ),
            "preview" => (
                Some("preview"),
                "合成入力の元値と補正結果を比較してください。",
            ),
            "confirmation" if saturated => (
                Some("preview"),
                "表示範囲を超えた値があるため確認できません。入力を変えて比較し直してください。",
            ),
            "confirmation" => (
                Some("confirm"),
                "7chの結果と未測定方向を確認してください。実機での有効性は確認されません。",
            ),
            _ if saved_current => (
                None,
                "この結果は記録済みです。実機の校正・制御利用には使えません。",
            ),
            _ => (
                Some("save"),
                "確認した合成結果を未検証記録として保存できます。",
            ),
        };
        let current_step = match phase {
            "not_started" => None,
            "baseline" => Some(0),
            "assessment" => Some(1),
            "preview" => Some(2),
            "confirmation" => Some(3),
            _ => Some(4),
        };
        let steps = ["基準", "応答・範囲", "入力比較", "結果確認", "記録"]
            .into_iter()
            .enumerate()
            .map(|(i, label)| Step {
                label,
                state: if saved_current || current_step.is_some_and(|n| i < n) {
                    "complete"
                } else if current_step == Some(i) {
                    "current"
                } else {
                    "pending"
                },
            })
            .collect();
        View {
            token: self.target(),
            id: self.selected.id().to_string(),
            role: self.selected,
            epoch: d.epoch,
            core_revision: d.session.as_ref().map(CalibrationSession::revision),
            phase,
            guidance,
            next_action,
            operations,
            steps,
            devices: [Role::Left, Role::Right]
                .into_iter()
                .map(|role| DeviceSummary {
                    role,
                    id: role.id().to_string(),
                    phase: self.devices[role.index()].phase(),
                })
                .collect(),
            rows: p
                .map(|p| {
                    (0..N)
                        .map(|i| Row {
                            wire: i,
                            physical: None,
                            firmware: p.firmware_sample[i],
                            baseline: p.baseline[i],
                            delta: p.delta[i],
                            sign: p.signs[i],
                            oriented: p.oriented[i],
                            positive_span: p.positive_spans[i],
                            normalized: p.normalized[i],
                            displayed: p.displayed[i],
                            saturated: p.saturated[i],
                            evidence: p.evidence[i].clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            history: d.history.as_ref().map(draft_json),
            saved_current,
            hardware_verified: false,
            control_ready: false,
        }
    }
    pub fn execute(&mut self, request: Request, store_root: &Path) -> Reply {
        if matches!(request.command, Command::Status {}) {
            return Reply {
                view: self.view(),
                error: None,
                export: None,
                message: String::new(),
            };
        }
        if request.expected.as_ref() != Some(&self.target()) {
            return Reply {
                view: self.view(),
                error: Some(Failure::new(
                    "stale_target",
                    "対象または作業状態が変わりました。現在の表示を確認して操作し直してください。",
                    "target/epoch/revision mismatch",
                )),
                export: None,
                message: String::new(),
            };
        }
        let Some(revision) = self.revision.checked_add(1) else {
            return Reply {
                view: self.view(),
                error: Some(Failure::condition("workspace revision exhausted")),
                export: None,
                message: String::new(),
            };
        };
        self.revision = revision;
        let result = self.apply(request.command, store_root);
        match result {
            Ok((message, export)) => Reply {
                view: self.view(),
                error: None,
                export,
                message,
            },
            Err(error) => Reply {
                view: self.view(),
                error: Some(error),
                export: None,
                message: String::new(),
            },
        }
    }
    fn apply(
        &mut self,
        command: Command,
        root: &Path,
    ) -> Result<(String, Option<String>), Failure> {
        if let Command::Select { role } = command {
            self.selected = role;
            return Ok((
                "表示対象を変更しました。各個体の調整状態は保持しています。".into(),
                None,
            ));
        }
        let role = self.selected;
        let d = &mut self.devices[role.index()];
        let message = match command {
            Command::Begin {} => {
                let epoch = d
                    .epoch
                    .checked_add(1)
                    .ok_or_else(|| Failure::condition("epoch exhausted"))?;
                d.session =
                    Some(CalibrationSession::new(role.context(epoch)).map_err(Failure::condition)?);
                d.epoch = epoch;
                d.saved = None;
                "この個体の合成調整を開始しました。"
            }
            Command::Baseline {} => {
                d.revoke_preview();
                d.session()?
                    .set_baseline([10.0; N])
                    .map_err(Failure::condition)?;
                "今回の合成基準を作成しました。"
            }
            Command::Assess { scenario } => {
                let s = d.session()?;
                s.revoke_candidates();
                if s.phase() == CalibrationPhase::NeedsBaseline {
                    return Err(Failure::condition("fresh baseline required"));
                }
                if ![
                    "good",
                    "noise",
                    "disagreement",
                    "unstable",
                    "release",
                    "crosstalk",
                    "timeout",
                    "direction",
                ]
                .contains(&scenario.as_str())
                {
                    return Err(Failure::new(
                        "invalid_scenario",
                        "未対応の合成条件です。応答確認をやり直してください。",
                        "unknown fixture scenario",
                    ));
                }
                let candidates: [Assessment; N] = std::array::from_fn(|i| {
                    assess(
                        i,
                        &trials(i, &scenario),
                        Thresholds {
                            noise_floor: 0.1,
                            stability: 0.5,
                            signal_to_noise: 5.0,
                            cross_ratio: 0.2,
                        },
                    )
                });
                let failures: Vec<_> = candidates
                    .iter()
                    .enumerate()
                    .filter_map(|(ch, result)| {
                        if let Assessment::Inconclusive(reason) = result {
                            Some(format!("ch {ch}: {reason}"))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !failures.is_empty() {
                    return Err(Failure::new(
                        "inconclusive",
                        "応答を確定できませんでした。合成条件と判定理由を確認し、応答確認をやり直してください。",
                        failures.join("; "),
                    ));
                }
                s.set_candidates(
                    candidates,
                    std::array::from_fn(|i| {
                        format!(
                            "synthetic trial wirech{i}; known direction; repeat 2; return checked"
                        )
                    }),
                )
                .map_err(Failure::condition)?;
                "合成観測の向き候補と正方向の操作範囲を確認しました。"
            }
            Command::Flip { channel, reason } => {
                d.revoke_preview();
                if reason.trim().is_empty() || reason.len() > 512 || channel >= N {
                    let mut error = Failure::new(
                        "manual_evidence",
                        "対象chと手動変更の根拠を確認してください。以前のプレビュー確認は失効しました。",
                        "channel 0..6 and nonempty reason up to 512 UTF-8 bytes required",
                    );
                    error.field = Some(if channel >= N {
                        "flip-channel"
                    } else {
                        "reason"
                    });
                    return Err(error);
                }
                d.session()?
                    .manual_flip(channel, &reason)
                    .map_err(Failure::condition)?;
                "向き候補を手動変更しました。入力を比較し直してください。"
            }
            Command::Preview { level } => {
                d.revoke_preview();
                let level: f64 = level.parse().map_err(|_| {
                    Failure::new(
                        "invalid_level",
                        "合成入力の値を確認してください。以前の確認は失効しました。",
                        "invalid fixture level",
                    )
                })?;
                if !level.is_finite() || !(-2.0..=2.0).contains(&level) {
                    return Err(Failure::new(
                        "invalid_level",
                        "合成入力は有限な−2〜2の範囲で指定してください。",
                        "fixture level out of bounds",
                    ));
                }
                let context = role.context(d.epoch);
                let sample = FirmwareSample::from_vector(
                    context,
                    &VectorFrame {
                        timestamp_ms: 0,
                        channels: std::array::from_fn(|i| {
                            10.0 + level
                                * if i % 2 == 0 {
                                    20.0 + i as f64 * 5.0
                                } else {
                                    -40.0 - i as f64 * 5.0
                                }
                        }),
                    },
                    Quality::KnownGoodSynthetic,
                )
                .map_err(Failure::condition)?;
                d.session()?.preview(&sample).map_err(Failure::condition)?;
                "元入力と補正結果を表示しました。未測定方向・表示範囲超過も確認してください。"
            }
            Command::Confirm {} => {
                let s = d.session()?;
                if s.phase() != CalibrationPhase::NeedsConfirmation {
                    return Err(Failure::condition("unconfirmed preview required"));
                }
                let p = s
                    .current_preview()
                    .cloned()
                    .ok_or_else(|| Failure::condition("preview required"))?;
                s.approve(&p).map_err(|e| Failure::new("confirmation_rejected", "このプレビューは確認できません。表示範囲を超える入力は比較し直してください。", e))?;
                "合成プレビューを確認しました。実機での有効性は未確認です。"
            }
            Command::Save {} => {
                if d.saved_current() {
                    return Err(Failure::new(
                        "already_saved",
                        "この結果は保存済みです。",
                        "duplicate save rejected",
                    ));
                }
                let draft = d.session()?.draft().map_err(Failure::condition)?;
                ProfileStore::new(root.to_owned())
                    .and_then(|s| s.save(&draft))
                    .map_err(|e| {
                        Failure::new(
                            "save_failed",
                            "記録を保存できませんでした。保存先または容量上限を確認してください。",
                            e,
                        )
                    })?;
                d.saved = d
                    .session
                    .as_ref()
                    .and_then(CalibrationSession::current_preview)
                    .cloned();
                "未検証の合成記録を保存しました。実機設定は変更していません。"
            }
            Command::Export {} => {
                let draft = d.session()?.draft().map_err(Failure::condition)?;
                let text = serde_json::to_string_pretty(&draft_json(&draft))
                    .map_err(|e| Failure::condition(e.to_string()))?;
                return Ok(("未検証の合成記録を書き出します。".into(), Some(text)));
            }
            Command::ReadHistory {} => {
                // Only the historical view changes, including on a failed read.
                d.history = None;
                let context = role.context(d.epoch.max(1));
                d.history = Some(ProfileStore::new(root.to_owned()).and_then(|s| s.load(&context))
                    .map_err(|e| Failure::new("history_read_failed", "保存記録を表示できませんでした。現在の調整状態は変更していません。", e))?);
                "この個体の最新記録を比較用に表示しました。現在の調整には適用していません。"
            }
            Command::Resume {} => {
                // Explicit, mutating restart: failure also invalidates the old session.
                if let Some(s) = d.session.as_mut() {
                    s.invalidate();
                }
                d.saved = None;
                let epoch = d
                    .epoch
                    .checked_add(1)
                    .ok_or_else(|| Failure::condition("epoch exhausted"))?;
                let context = role.context(epoch);
                let history = d
                    .history
                    .as_ref()
                    .ok_or_else(|| Failure::condition("read history before resuming"))?;
                let session = resume(history, context).map_err(|e| {
                    Failure::new(
                        "resume_failed",
                        "保存記録が今回の条件に適合しません。基準から確認し直してください。",
                        e,
                    )
                })?;
                d.epoch = epoch;
                d.session = Some(session);
                "記録と今回の条件を照合して調整を再開しました。基準・向き・範囲は復元せず、再観測が必要です。"
            }
            Command::Invalidate {} => {
                d.session()?.invalidate();
                "合成sessionを失効させました。実機へtare・resetを送信していません。"
            }
            Command::Status {} | Command::Select { .. } => {
                unreachable!("handled before device mutation")
            }
        };
        Ok((message.into(), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };
    // The first path is deliberately absent until a test saves a record.
    // The second path is an exclusively claimed parent owned by this guard.
    struct Store(PathBuf, PathBuf);
    impl Store {
        fn new() -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            Self::new_in(&std::env::temp_dir(), &SEQUENCE)
        }
        fn new_in(parent: &Path, sequence: &AtomicU64) -> Self {
            for _ in 0..128 {
                // Candidate numbers may wrap; exclusive creation still prevents sharing.
                let serial = sequence.fetch_add(1, Ordering::Relaxed);
                let reservation =
                    parent.join(format!("srn-workflow-{}-{serial}", std::process::id()));
                match fs::create_dir(&reservation) {
                    Ok(()) => return Self(reservation.join("records"), reservation),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot reserve test store: {error}"),
                }
            }
            panic!("test store reservation attempts exhausted");
        }
    }
    impl Drop for Store {
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.1) {
                if std::thread::panicking() {
                    // Preserve the primary failure instead of aborting on a second panic.
                    let _ = writeln!(std::io::stderr(), "test store cleanup failed: {error}");
                } else {
                    panic!("test store cleanup failed: {error}");
                }
            }
        }
    }
    #[test]
    fn store_reservation_retries_existing_names_without_touching_them() {
        let owner = Store::new();
        fs::create_dir(&owner.0).unwrap();
        let first = Store::new_in(&owner.0, &AtomicU64::new(0));
        let marker = first.1.join("owned-by-first");
        fs::write(&marker, "unchanged").unwrap();
        // Reuse exactly the same candidate sequence, like a reused process ID.
        let second = Store::new_in(&owner.0, &AtomicU64::new(0));
        assert_ne!(first.1, second.1);
        assert!(!first.0.exists());
        assert!(!second.0.exists());
        let removed = second.1.clone();
        drop(second);
        assert!(!removed.exists());
        let rollover = AtomicU64::new(u64::MAX);
        let last = Store::new_in(&owner.0, &rollover);
        let wrapped = Store::new_in(&owner.0, &rollover);
        assert_ne!(last.1, wrapped.1);
        assert_ne!(first.1, wrapped.1);
        assert_eq!(fs::read_to_string(marker).unwrap(), "unchanged");
    }
    #[test]
    fn parallel_stores_keep_records_and_cleanup_independent() {
        let owner = Store::new();
        fs::create_dir(&owner.0).unwrap();
        let sequence = AtomicU64::new(0);
        let mut stores = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..16)
                .map(|_| scope.spawn(|| Store::new_in(&owner.0, &sequence)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        let paths: std::collections::HashSet<_> = stores.iter().map(|s| &s.1).collect();
        assert_eq!(paths.len(), stores.len());
        for (i, store) in stores.iter().enumerate() {
            fs::create_dir(&store.0).unwrap();
            fs::write(store.0.join("record"), i.to_string()).unwrap();
        }
        let removed = stores.pop().unwrap();
        let removed_path = removed.1.clone();
        drop(removed);
        assert!(!removed_path.exists());
        for (i, store) in stores.iter().enumerate() {
            assert_eq!(
                fs::read_to_string(store.0.join("record")).unwrap(),
                i.to_string()
            );
        }
    }
    #[test]
    fn cleanup_error_during_unwind_preserves_primary_failure() {
        let owner = Store::new();
        fs::create_dir(&owner.0).unwrap();
        let store = Store::new_in(&owner.0, &AtomicU64::new(0));
        // Make cleanup fail deterministically, only inside this test's reservation.
        fs::remove_dir(&store.1).unwrap();
        fs::write(&store.1, "not a directory").unwrap();
        let failed_cleanup_path = store.1.clone();
        let failure = std::panic::catch_unwind(move || {
            let _store = store;
            panic!("primary test failure");
        })
        .unwrap_err();
        assert_eq!(
            failure.downcast_ref::<&str>(),
            Some(&"primary test failure")
        );
        assert!(failed_cleanup_path.is_file(), "cleanup must actually fail");
    }
    fn run(w: &mut Workspace, store: &Store, command: Command) -> Reply {
        w.execute(
            Request {
                expected: Some(w.target()),
                command,
            },
            &store.0,
        )
    }
    fn ok(w: &mut Workspace, store: &Store, command: Command) -> Reply {
        let r = run(w, store, command);
        assert!(r.error.is_none(), "{:?}", r.error);
        r
    }
    fn approved(w: &mut Workspace, store: &Store) {
        for command in [
            Command::Begin {},
            Command::Baseline {},
            Command::Assess {
                scenario: "good".into(),
            },
            Command::Preview {
                level: "0.5".into(),
            },
            Command::Confirm {},
        ] {
            ok(w, store, command);
        }
    }
    fn can(view: &View, id: &str) -> bool {
        view.operations.iter().find(|o| o.id == id).unwrap().enabled
    }
    #[test]
    fn history_read_and_missing_history_leave_active_session_unchanged() {
        let (mut w, store) = (Workspace::default(), Store::new());
        approved(&mut w, &store);
        let before = w.view();
        assert!(run(&mut w, &store, Command::ReadHistory {}).error.is_some());
        let after = w.view();
        assert_eq!(before.epoch, after.epoch);
        assert_eq!(before.core_revision, after.core_revision);
        assert_eq!(after.phase, "record");
        assert!(can(&after, "save"));
        assert_eq!(after.rows.len(), N);
        assert!(!store.0.exists());
        ok(&mut w, &store, Command::Save {});
        let before = w.view();
        let after = ok(&mut w, &store, Command::ReadHistory {}).view;
        assert_eq!(before.epoch, after.epoch);
        assert_eq!(before.core_revision, after.core_revision);
        assert!(after.saved_current);
        assert_eq!(after.phase, "record");
        assert!(can(&after, "export"));
        assert!(after.history.is_some());
        assert_eq!(after.rows[0].normalized, Some(0.5));
    }
    #[test]
    fn corrupt_history_clears_only_history_not_active_confirmation() {
        let (mut w, store) = (Workspace::default(), Store::new());
        approved(&mut w, &store);
        ok(&mut w, &store, Command::Save {});
        ok(&mut w, &store, Command::ReadHistory {});
        let directory = store.0.join(Role::Left.id().to_string());
        let path = fs::read_dir(directory)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(path, "broken").unwrap();
        let r = run(&mut w, &store, Command::ReadHistory {});
        assert_eq!(r.error.unwrap().code, "history_read_failed");
        assert!(r.view.history.is_none());
        assert_eq!(r.view.phase, "record");
        assert!(can(&r.view, "export"));
    }
    #[test]
    fn explicit_resume_and_failed_resume_revoke_confirmation() {
        let (mut w, store) = (Workspace::default(), Store::new());
        approved(&mut w, &store);
        assert!(run(&mut w, &store, Command::Resume {}).error.is_some());
        assert_eq!(w.view().phase, "baseline");
        assert!(!can(&w.view(), "save"));
        approved(&mut w, &store);
        ok(&mut w, &store, Command::Save {});
        ok(&mut w, &store, Command::ReadHistory {});
        let epoch = w.view().epoch;
        let r = ok(&mut w, &store, Command::Resume {});
        assert_eq!(r.view.epoch, epoch + 1);
        assert_eq!(r.view.phase, "baseline");
        assert!(r.view.rows.is_empty());
        assert!(r.view.history.is_some());
        assert!(!can(&r.view, "preview"));
    }
    #[test]
    fn selection_preserves_per_device_sessions_and_rejects_stale_requests() {
        let (mut w, store) = (Workspace::default(), Store::new());
        approved(&mut w, &store);
        let left = w.view();
        ok(&mut w, &store, Command::Select { role: Role::Right });
        assert_eq!(w.view().phase, "not_started");
        let r = w.execute(
            Request {
                expected: Some(left.token),
                command: Command::Invalidate {},
            },
            &store.0,
        );
        assert_eq!(r.error.unwrap().code, "stale_target");
        assert_eq!(r.view.phase, "not_started");
        approved(&mut w, &store);
        ok(&mut w, &store, Command::Select { role: Role::Left });
        assert_eq!(w.view().epoch, left.epoch);
        assert_eq!(w.view().phase, "record");
        assert_eq!(w.view().core_revision, left.core_revision);
    }
    #[test]
    fn same_request_cannot_be_replayed() {
        let (mut w, store) = (Workspace::default(), Store::new());
        let target = w.target();
        ok(&mut w, &store, Command::Begin {});
        let r = w.execute(
            Request {
                expected: Some(target),
                command: Command::Begin {},
            },
            &store.0,
        );
        assert_eq!(r.error.unwrap().code, "stale_target");
        assert_eq!(r.view.epoch, 1);
    }
    #[test]
    fn invalid_mutation_values_revoke_confirmation_and_failed_assess_revokes_candidates() {
        let (mut w, store) = (Workspace::default(), Store::new());
        for command in [
            Command::Preview {
                level: "NaN".into(),
            },
            Command::Preview {
                level: "bad".into(),
            },
            Command::Preview { level: "3".into() },
            Command::Flip {
                channel: 0,
                reason: "".into(),
            },
            Command::Flip {
                channel: 99,
                reason: "reason".into(),
            },
            Command::Assess {
                scenario: "unknown".into(),
            },
        ] {
            approved(&mut w, &store);
            let r = run(&mut w, &store, command);
            assert!(r.error.is_some());
            assert!(r.view.rows.is_empty());
            assert!(!can(&r.view, "save"));
            assert!(!can(&r.view, "export"));
        }
        assert!(!can(&w.view(), "flip"));
        assert!(!can(&w.view(), "preview"));
    }
    #[test]
    fn all_quality_failures_block_old_candidates() {
        let (mut w, store) = (Workspace::default(), Store::new());
        for scenario in [
            "noise",
            "disagreement",
            "unstable",
            "release",
            "crosstalk",
            "timeout",
            "direction",
        ] {
            approved(&mut w, &store);
            let r = run(
                &mut w,
                &store,
                Command::Assess {
                    scenario: scenario.into(),
                },
            );
            assert_eq!(r.error.unwrap().code, "inconclusive");
            assert_eq!(r.view.phase, "assessment");
            assert!(!can(&r.view, "preview"));
            assert!(!can(&r.view, "save"));
        }
    }
    #[test]
    fn bounds_unknown_reverse_and_unmapped_physical_positions_remain_explicit() {
        let (mut w, store) = (Workspace::default(), Store::new());
        approved(&mut w, &store);
        let v = ok(
            &mut w,
            &store,
            Command::Preview {
                level: "1.5".into(),
            },
        )
        .view;
        assert!(v.rows.iter().all(|r| r.physical.is_none()
            && r.normalized == Some(1.5)
            && r.displayed == Some(1.0)
            && r.saturated));
        assert!(!can(&v, "confirm"));
        assert!(!v.hardware_verified);
        assert!(!v.control_ready);
        assert!(run(&mut w, &store, Command::Confirm {}).error.is_some());
        let v = ok(
            &mut w,
            &store,
            Command::Preview {
                level: "-0.5".into(),
            },
        )
        .view;
        assert!(
            v.rows
                .iter()
                .all(|r| r.normalized.is_none() && r.displayed.is_none())
        );
    }
    #[test]
    fn typed_requests_reject_unknown_or_unstructured_fields() {
        assert!(
            serde_json::from_str::<Request>(
                r#"{"expected":null,"command":{"action":"flip","argument":"0:why"}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Request>(
                r#"{"expected":null,"command":{"action":"select","role":"other"}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Request>(
                r#"{"expected":null,"command":{"action":"status","arbitrary":true}}"#
            )
            .is_err()
        );
    }
}
