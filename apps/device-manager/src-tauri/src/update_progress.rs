//! 書込み結果とは独立した、有界で読み取り専用の進捗スナップショット。
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    #[default]
    Idle,
    Checking,
    Resetting,
    Bootloader,
    AwaitingConfirmation,
    Programming,
    Verifying,
    Reconnecting,
    Complete,
    NeedsRecovery,
    Failed,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Snapshot {
    pub operation_id: String,
    pub sequence: u64,
    pub target: String,
    pub stage: Stage,
    pub percent: Option<u8>,
    pub active: bool,
}
#[derive(Clone, Default)]
pub struct Progress(Arc<Mutex<Snapshot>>);
impl Progress {
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.0
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "更新進捗を取得できません。".into())
    }
    pub fn active(&self) -> bool {
        self.0.lock().map(|s| s.active).unwrap_or(true)
    }
    pub fn begin(&self, id: &str, target: &str) {
        if let Ok(mut s) = self.0.lock() {
            s.operation_id = id.to_owned();
            s.target = target.to_owned();
            s.sequence = s.sequence.saturating_add(1);
            s.stage = Stage::Checking;
            s.percent = None;
            s.active = true;
        }
    }
    pub fn stage(&self, stage: Stage, percent: Option<u8>) {
        if let Ok(mut s) = self.0.lock() {
            if !s.active {
                return;
            }
            s.sequence = s.sequence.saturating_add(1);
            s.stage = stage;
            s.percent = percent.filter(|v| *v <= 100);
        }
    }
    pub fn finish(&self, stage: Stage) {
        if let Ok(mut s) = self.0.lock() {
            s.sequence = s.sequence.saturating_add(1);
            s.stage = stage;
            s.percent = if stage == Stage::Complete {
                Some(100)
            } else {
                None
            };
            s.active = false;
        }
    }
}
/// AVRDUDEの出力は分割read/CR更新に対応。未認識行から割合を推測しない。
pub struct ProgrammerProgress {
    progress: Progress,
    line: Vec<u8>,
    overflow: bool,
    wrote: bool,
}
impl ProgrammerProgress {
    pub fn new(progress: Progress) -> Self {
        Self {
            progress,
            line: Vec::with_capacity(256),
            overflow: false,
            wrote: false,
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if matches!(byte, b'\r' | b'\n') {
                if !self.overflow {
                    self.line_complete();
                }
                self.line.clear();
                self.overflow = false;
            } else if self.line.len() < 1024 {
                self.line.push(byte);
            } else {
                self.overflow = true;
            }
        }
    }
    fn line_complete(&mut self) {
        let Ok(line) = std::str::from_utf8(&self.line) else {
            return;
        };
        let line = line.trim();
        let stage = if line.starts_with("Writing |") {
            self.wrote = true;
            Stage::Programming
        } else if self.wrote && line.starts_with("Reading |") {
            Stage::Verifying
        } else {
            return;
        };
        let percent = line
            .split_whitespace()
            .find_map(|part| part.strip_suffix('%')?.parse::<u8>().ok())
            .filter(|p| *p <= 100);
        self.progress.stage(stage, percent);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_carriage_returns_and_oversized_lines_never_fake_completion() {
        let p = Progress::default();
        p.begin("plan-a", "COM1");
        let mut parser = ProgrammerProgress::new(p.clone());
        parser.feed(b"Writ");
        parser.feed(b"ing | #### | 40% 0.1s\r");
        assert_eq!(p.snapshot().unwrap().percent, Some(40));
        parser.feed(b"Reading | ## | 100% 0.2s\n");
        assert_eq!(p.snapshot().unwrap().stage, Stage::Verifying);
        assert!(p.active());
        parser.feed(&vec![b'x'; 2048]);
        parser.feed(b"Writing | | 90%\n");
        assert_eq!(p.snapshot().unwrap().stage, Stage::Verifying);
        parser.feed(b"Writing | | 255%\n");
        assert_eq!(p.snapshot().unwrap().percent, None);
        p.finish(Stage::NeedsRecovery);
        parser.feed(b"Writing | | 100%\n");
        assert_eq!(p.snapshot().unwrap().stage, Stage::NeedsRecovery);
        assert_eq!(p.snapshot().unwrap().percent, None);
    }
    #[test]
    fn new_attempt_has_new_sequence_and_only_checked_success_is_complete() {
        let p = Progress::default();
        p.begin("a", "COM2");
        p.finish(Stage::AwaitingConfirmation);
        let previous = p.snapshot().unwrap().sequence;
        p.begin("a", "COM3");
        assert!(p.snapshot().unwrap().sequence > previous);
        assert_eq!(p.snapshot().unwrap().percent, None);
        p.finish(Stage::Complete);
        assert_eq!(p.snapshot().unwrap().percent, Some(100));
        assert!(!p.active());
    }
}
