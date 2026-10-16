//! Fail-closed firmware package validation and ATmega32U4 flash orchestration.
//!
//! The UI can select a USB target and provide package text, but it cannot pick
//! an executable, programmer, MCU, memory region, or command-line argument.

use crate::discovery::{PortKey, PortView};
use selfrionette_serial::{
    LineTransport, SerialPortTransport, TransportError, available_port_summaries,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const APP_RELEASE_VERSION: &str = env!("CARGO_PKG_VERSION");
const APPLICATION: &str = "selfrionette-loadcell-7ch";
const BOARD: &str = "sparkfun_promicro16";
const MCU: &str = "atmega32u4";
const CLOCK_HZ: u32 = 16_000_000;
const PROTOCOL_SCHEMA: u8 = 3;
const SETTINGS_SCHEMA: u8 = 1;
const APP_FLASH_START: u32 = 0;
const APP_FLASH_END_EXCLUSIVE: u32 = 0x7000;
const MAX_MANIFEST_BYTES: usize = 8 * 1024;
const MAX_HEX_BYTES: usize = 512 * 1024;
const MAX_HEX_RECORDS: usize = 4096;
const MAX_DEVICE_LINES: usize = 32;
const INFO_TIMEOUT: Duration = Duration::from_millis(1800);
const POST_FLASH_TIMEOUT: Duration = Duration::from_secs(24);
const PROGRAMMER_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_PROGRAMMER_OUTPUT: usize = 32 * 1024;
const MAX_JOURNAL_BYTES: u64 = 64 * 1024;
const MANAGER_UPDATE_DIR: &str = "firmware-updates";
const PENDING_JOURNAL: &str = "pending-update.json";

const EXPECTED_FIRMWARE_MANIFEST_SHA256: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_FIRMWARE_MANIFEST_SHA256");
const EXPECTED_FIRMWARE_IMAGE_SHA256: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_FIRMWARE_IMAGE_SHA256");
const EXPECTED_AVRDUDE_EXE_SHA256: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_AVRDUDE_EXE_SHA256");
const EXPECTED_AVRDUDE_CONF_SHA256: Option<&str> =
    option_env!("SELFRIONETTE_EXPECTED_AVRDUDE_CONF_SHA256");
const EXPECTED_SOURCE_SHA: Option<&str> = option_env!("SELFRIONETTE_SOURCE_SHA");

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SelectedPackageInput {
    pub manifest_json: String,
    pub firmware_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareRequest {
    pub target: PortKey,
    /// `None` selects the release package embedded in the installed application.
    /// `Some` is local user input; its source provenance is never inferred from
    /// a field in the package itself.
    pub package: Option<SelectedPackageInput>,
    /// Required when the currently installed firmware cannot identify itself as
    /// the supported board with protocol 3 metadata.
    #[serde(default)]
    pub manual_board_confirmation: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub plan_id: String,
    pub operator_confirmed: bool,
    /// Supplied only after reset when topology cannot bind the bootloader port.
    #[serde(default)]
    pub bootloader_target: Option<PortKey>,
    #[serde(default)]
    pub manual_bootloader_confirmation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FirmwarePackageManifest {
    pub schema: u8,
    pub release_version: String,
    pub application: String,
    pub board: String,
    pub mcu: String,
    pub clock_hz: u32,
    pub protocol_schema: u8,
    pub settings_schema: u8,
    pub firmware_version: String,
    pub source_sha: String,
    pub image_file: String,
    pub image_sha256: String,
    pub image_size: u64,
    pub flash_start: u32,
    pub flash_end_exclusive: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceInfo3 {
    pub firmware_version: String,
    pub source_sha: String,
    pub board: String,
    pub mcu: String,
    pub clock_hz: u32,
    pub device_id: String,
    pub generation: u32,
    pub assembly: u32,
    pub known: u8,
    pub negative: u8,
    pub tare_epoch: u32,
    pub tare_valid: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LegacyDeviceInfo {
    pub firmware_version: String,
    pub device_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceObservation {
    Protocol3 { info: DeviceInfo3 },
    Legacy { info: LegacyDeviceInfo },
    Unidentified,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrepareReply {
    pub status: &'static str,
    pub plan_id: Option<String>,
    pub target: PortKey,
    pub observation: DeviceObservation,
    pub manifest: FirmwarePackageManifest,
    pub manifest_sha256: String,
    pub firmware_sha256: String,
    pub source_trust: &'static str,
    pub backup_id: Option<String>,
    pub backup_status: &'static str,
    pub topology: TopologySummary,
    pub manual_board_confirmation_required: bool,
    pub manual_board_confirmation_satisfied: bool,
    pub operator_confirmation_required: bool,
    pub apply_enabled: bool,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TopologySummary {
    pub selected_port_matches: usize,
    pub other_candidate_count: usize,
    pub stable_usb_serial_available: bool,
    pub location_path_available: bool,
    pub status: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct ApplyReply {
    pub status: &'static str,
    pub phase: &'static str,
    pub needs_recovery: bool,
    pub manual_bootloader_confirmation_required: bool,
    pub bootloader_candidates: Vec<PortKey>,
    pub backup_id: String,
    pub firmware_version: Option<String>,
    pub source_sha: Option<String>,
    pub device_id: Option<String>,
    pub settings_preserved: Option<bool>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecoveryReply {
    pub status: &'static str,
    pub needs_recovery: bool,
    pub backup_id: Option<String>,
    pub firmware_version: Option<String>,
    pub source_sha: Option<String>,
    pub device_id: Option<String>,
    pub settings_preserved: Option<bool>,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BackupRecord {
    pub schema: u8,
    pub backup_id: String,
    pub captured_unix_seconds: u64,
    pub target: PortKey,
    pub observation: DeviceObservation,
    pub settings_snapshot: Option<SettingsSnapshot>,
    pub settings_backup_status: &'static str,
    pub firmware_package_sha256: String,
    pub source_trust: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SettingsSnapshot {
    pub generation: u32,
    pub assembly: u32,
    pub known: u8,
    pub negative: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryJournal {
    schema: u8,
    plan_id: String,
    target: PortKey,
    binding: TargetBinding,
    before: DeviceObservation,
    manifest: FirmwarePackageManifest,
    manifest_sha256: String,
    firmware_sha256: String,
    backup_id: String,
    source_trust: String,
    #[serde(default = "legacy_journal_may_have_written")]
    write_may_have_started: bool,
    created_unix_seconds: u64,
}

#[derive(Clone, Debug)]
struct PreparedPackage {
    manifest: FirmwarePackageManifest,
    manifest_sha256: String,
    firmware_sha256: String,
    firmware_hex_path: PathBuf,
    source_trust: &'static str,
}

#[derive(Clone, Debug)]
struct PendingPlan {
    plan_id: String,
    target: PortKey,
    binding: TargetBinding,
    before: DeviceObservation,
    package: PreparedPackage,
    backup_id: String,
    manual_board_confirmed: bool,
    topology: TopologySummary,
    reset_completed: bool,
    awaiting_bootloader_confirmation: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TargetBinding {
    location_path: Option<String>,
    serial_number: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PortRecord {
    key: PortKey,
    location_path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProgrammerRuntime {
    executable: PathBuf,
    config: PathBuf,
}

#[derive(Clone, Debug)]
struct RuntimePins {
    executable_sha256: Option<String>,
    config_sha256: Option<String>,
}

impl RuntimePins {
    fn from_build() -> Self {
        Self {
            executable_sha256: EXPECTED_AVRDUDE_EXE_SHA256.map(str::to_owned),
            config_sha256: EXPECTED_AVRDUDE_CONF_SHA256.map(str::to_owned),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FlashError {
    pub writes_may_have_started: bool,
    pub message: String,
}

/// Injection boundary used by software-only tests. The production backend
/// opens only the selected USB serial port and invokes the fixed avrdude flow.
trait Backend: Send {
    fn ports(&mut self) -> Result<Vec<PortRecord>, String>;
    fn inspect(&mut self, target: &PortKey) -> Result<DeviceObservation, String>;
    fn reset_to_bootloader(&mut self, target: &PortKey) -> Result<(), FlashError>;
    fn flash(
        &mut self,
        target: &PortKey,
        image_path: &Path,
        runtime: &ProgrammerRuntime,
    ) -> Result<(), FlashError>;
}

pub struct UpdaterService {
    resource_dir: PathBuf,
    app_config_dir: PathBuf,
    backend: Box<dyn Backend>,
    runtime_pins: RuntimePins,
    pending_plan: Option<PendingPlan>,
}

impl UpdaterService {
    pub fn new(resource_dir: PathBuf, app_config_dir: PathBuf) -> Self {
        Self::with_backend(resource_dir, app_config_dir, Box::new(SystemBackend))
    }

    fn with_backend(
        resource_dir: PathBuf,
        app_config_dir: PathBuf,
        backend: Box<dyn Backend>,
    ) -> Self {
        Self::with_backend_and_runtime_pins(
            resource_dir,
            app_config_dir,
            backend,
            RuntimePins::from_build(),
        )
    }

    fn with_backend_and_runtime_pins(
        resource_dir: PathBuf,
        app_config_dir: PathBuf,
        backend: Box<dyn Backend>,
        runtime_pins: RuntimePins,
    ) -> Self {
        Self {
            resource_dir,
            app_config_dir,
            backend,
            runtime_pins,
            pending_plan: None,
        }
    }

    pub fn prepare(&mut self, request: PrepareRequest) -> Result<PrepareReply, String> {
        let pending_path = self.pending_journal_path();
        if pending_path.exists() {
            return Err(
                "A previous flash may be incomplete. Run recovery read-back before another update."
                    .into(),
            );
        }
        // Explicit preparation replaces only a plan which has not entered reset/write.
        // The persistent recovery marker above forbids replacing an in-flight update.
        self.pending_plan = None;

        validate_target_key(&request.target)?;
        let package = self.load_package(request.package.as_ref())?;
        let ports = self.backend.ports()?;
        let (topology, binding) = validate_topology(&ports, &request.target)?;
        let before = self.backend.inspect(&request.target)?;
        validate_observation(&before)?;
        validate_board_if_reported(&before)?;

        let manual_required = !matches!(before, DeviceObservation::Protocol3 { .. });
        if manual_required && !request.manual_board_confirmation {
            return Ok(PrepareReply {
                status: "requires_manual_board_confirmation",
                plan_id: None,
                target: request.target,
                observation: before,
                manifest: package.manifest,
                manifest_sha256: package.manifest_sha256,
                firmware_sha256: package.firmware_sha256,
                source_trust: package.source_trust,
                backup_id: None,
                backup_status: "not_created",
                topology,
                manual_board_confirmation_required: true,
                manual_board_confirmation_satisfied: false,
                operator_confirmation_required: true,
                apply_enabled: false,
                message: "The installed firmware did not provide complete protocol 3 board metadata. Confirm the physical Pro Micro / ATmega32U4 16 MHz board, then prepare again.".into(),
            });
        }

        if let DeviceObservation::Protocol3 { info } = &before
            && (info.board != package.manifest.board
                || info.mcu != package.manifest.mcu
                || info.clock_hz != package.manifest.clock_hz)
        {
            return Err(
                "The reported board, MCU, or clock does not match the firmware package.".into(),
            );
        }

        let plan_id = new_id("plan");
        let backup_id = new_id("backup");
        let backup =
            make_backup_record(&backup_id, request.target.clone(), before.clone(), &package);
        persist_backup(&self.app_config_dir, &backup)?;
        let backup_status = backup.settings_backup_status;
        self.pending_plan = Some(PendingPlan {
            plan_id: plan_id.clone(),
            target: request.target.clone(),
            binding,
            before: before.clone(),
            package: package.clone(),
            backup_id: backup_id.clone(),
            manual_board_confirmed: !manual_required || request.manual_board_confirmation,
            topology: topology.clone(),
            reset_completed: false,
            awaiting_bootloader_confirmation: false,
        });

        Ok(PrepareReply {
            status: "ready",
            plan_id: Some(plan_id),
            target: request.target,
            observation: before,
            manifest: package.manifest,
            manifest_sha256: package.manifest_sha256,
            firmware_sha256: package.firmware_sha256,
            source_trust: package.source_trust,
            backup_id: Some(backup_id),
            backup_status,
            topology,
            manual_board_confirmation_required: manual_required,
            manual_board_confirmation_satisfied: !manual_required || request.manual_board_confirmation,
            operator_confirmation_required: true,
            apply_enabled: true,
            message: "Package, target topology, and pre-update snapshot are ready. Flashing still requires the separate operator confirmation.".into(),
        })
    }

    pub fn apply(&mut self, request: ApplyRequest) -> Result<ApplyReply, String> {
        let mut plan = self.pending_plan.clone().ok_or_else(|| {
            "No prepared update is available. Prepare the target and package again.".to_string()
        })?;
        if request.plan_id != plan.plan_id {
            return Err(
                "The prepared update no longer matches this request. Prepare again.".into(),
            );
        }
        if !request.operator_confirmed {
            return Err("Flashing requires the operator's explicit confirmation after reviewing the target, package, backup, and trust status.".into());
        }
        if !plan.manual_board_confirmed {
            return Err(
                "Manual board confirmation is required before flashing this firmware.".into(),
            );
        }
        let journal_path = self.pending_journal_path();
        if plan.reset_completed != journal_path.exists() {
            return Err("The update recovery marker and prepared stage disagree. Keep the device connected and recover before proceeding.".into());
        }
        if !plan.reset_completed {
            if request.bootloader_target.is_some() || request.manual_bootloader_confirmation {
                return Err("A bootloader target can only be confirmed after the application port has reset and re-enumerated.".into());
            }
            let ports = self.backend.ports()?;
            let (topology, binding) = validate_topology(&ports, &plan.target)?;
            if binding != plan.binding || topology.status != plan.topology.status {
                return Err("USB location or serial topology changed after preparation. Stop and prepare the target again.".into());
            }
            let latest = self.backend.inspect(&plan.target)?;
            validate_observation(&latest)?;
            if latest != plan.before {
                return Err("Device identity, firmware, or settings changed after preparation. Stop and prepare the target again.".into());
            }

            let image_bytes = read_bounded(&plan.package.firmware_hex_path, MAX_HEX_BYTES)?;
            let image_text = std::str::from_utf8(&image_bytes).map_err(|_| {
                "The staged firmware image is not valid UTF-8 Intel HEX.".to_string()
            })?;
            if sha256(&image_bytes) != plan.package.firmware_sha256 {
                return Err("The staged firmware image changed after preparation.".into());
            }
            validate_manifest_and_hex(
                &plan.package.manifest,
                image_bytes.len() as u64,
                image_text,
            )?;
            verify_runtime_hashes(&self.resource_dir, &self.runtime_pins)?;

            let journal = RecoveryJournal {
                schema: 3,
                plan_id: plan.plan_id.clone(),
                target: plan.target.clone(),
                binding: plan.binding.clone(),
                before: plan.before.clone(),
                manifest: plan.package.manifest.clone(),
                manifest_sha256: plan.package.manifest_sha256.clone(),
                firmware_sha256: plan.package.firmware_sha256.clone(),
                backup_id: plan.backup_id.clone(),
                source_trust: plan.package.source_trust.into(),
                write_may_have_started: false,
                created_unix_seconds: unix_seconds(),
            };
            self.write_pending_journal(&journal)?;

            if let Err(error) = self.backend.reset_to_bootloader(&plan.target) {
                self.pending_plan = None;
                return Ok(recovery_apply_reply(
                    "reset",
                    plan.backup_id,
                    Vec::new(),
                    format!(
                        "The bootloader reset did not complete cleanly. No programmer was launched; keep the selected board connected and use original-state read-back recovery before retrying: {}",
                        error.message
                    ),
                ));
            }
            plan.reset_completed = true;
            self.pending_plan = Some(plan.clone());
        }

        let ports = match wait_for_bootloader(&mut *self.backend, &plan.target, &plan.binding) {
            Ok(ports) => ports,
            Err(error) => {
                self.pending_plan = None;
                return Ok(recovery_apply_reply(
                    "bootloader_enumeration",
                    plan.backup_id,
                    Vec::new(),
                    format!(
                        "No unique bootloader target was confirmed after reset. Keep the device connected and use read-back recovery: {error}"
                    ),
                ));
            }
        };
        let bootloader_candidates: Vec<PortKey> = ports
            .iter()
            .filter(|port| port_category(&port.key) == "bootloader")
            .map(|port| port.key.clone())
            .collect();
        let selected = resolve_bootloader_target(
            &ports,
            &plan.target,
            &plan.binding,
            request.bootloader_target.as_ref(),
            request.manual_bootloader_confirmation,
            plan.awaiting_bootloader_confirmation,
        );
        let bootloader = match selected {
            Ok(BootloaderSelection::Ready(target)) => target,
            Ok(BootloaderSelection::NeedsManualConfirmation(candidates)) => {
                plan.awaiting_bootloader_confirmation = true;
                self.pending_plan = Some(plan.clone());
                return Ok(ApplyReply {
                    status: "awaiting_bootloader_confirmation",
                    phase: "bootloader_selection",
                    needs_recovery: false,
                    manual_bootloader_confirmation_required: true,
                    bootloader_candidates: candidates,
                    backup_id: plan.backup_id,
                    firmware_version: None,
                    source_sha: None,
                    device_id: None,
                    settings_preserved: None,
                    message: "The application reset and the device uniquely re-enumerated as a bootloader, but Windows did not expose a stable location path or serial number. Confirm this exact bootloader port before flashing.".into(),
                });
            }
            Err(error) => {
                self.pending_plan = None;
                return Ok(recovery_apply_reply(
                    "bootloader_selection",
                    plan.backup_id,
                    bootloader_candidates,
                    format!(
                        "The bootloader target is ambiguous or unknown. No flash was started; keep the device connected and use read-back recovery: {error}"
                    ),
                ));
            }
        };

        if !plan.awaiting_bootloader_confirmation && request.bootloader_target.is_some() {
            return Err("An explicit bootloader port can only be supplied after the service requested manual confirmation.".into());
        }
        let staged = (|| -> Result<(), String> {
            let image_bytes = read_bounded(&plan.package.firmware_hex_path, MAX_HEX_BYTES)?;
            let image_text = std::str::from_utf8(&image_bytes).map_err(|_| {
                "The staged firmware image is not valid UTF-8 Intel HEX.".to_string()
            })?;
            if sha256(&image_bytes) != plan.package.firmware_sha256 {
                return Err("The staged firmware image changed after reset.".into());
            }
            validate_manifest_and_hex(
                &plan.package.manifest,
                image_bytes.len() as u64,
                image_text,
            )?;
            verify_runtime_hashes(&self.resource_dir, &self.runtime_pins)?;
            Ok(())
        })();
        if let Err(error) = staged {
            self.pending_plan = None;
            return Ok(recovery_apply_reply(
                "pre_flash_validation",
                plan.backup_id,
                bootloader_candidates,
                format!(
                    "Pre-flash validation failed after the bootloader reset. No flash was started; recovery read-back remains required: {error}"
                ),
            ));
        }
        let runtime = runtime_paths(&self.resource_dir);

        let target = bootloader;
        let image_path = plan.package.firmware_hex_path.clone();
        let backup_id = plan.backup_id.clone();
        let before = plan.before.clone();
        let binding = plan.binding.clone();
        let manifest = plan.package.manifest.clone();
        if let Err(error) = self.persist_programmer_start(&plan) {
            self.pending_plan = None;
            return Ok(recovery_apply_reply(
                "journal_write_stage",
                plan.backup_id,
                bootloader_candidates,
                format!(
                    "The durable programmer-start marker could not be persisted, so the programmer was not launched. Keep the device connected and use original-state read-back recovery: {error}"
                ),
            ));
        }
        self.pending_plan = None;

        if let Err(error) = self.backend.flash(&target, &image_path, &runtime) {
            return Ok(ApplyReply {
                status: "needs_recovery",
                phase: "flash_failed",
                needs_recovery: true,
                manual_bootloader_confirmation_required: false,
                bootloader_candidates,
                backup_id,
                firmware_version: None,
                source_sha: None,
                device_id: None,
                settings_preserved: None,
                message: format!(
                    "Flash did not complete (writes may have started: {}). Keep the device connected and run recovery read-back before retrying: {}",
                    error.writes_may_have_started, error.message
                ),
            });
        }

        match self.verify_after_flash(&binding, &before, &manifest) {
            Ok(readback) => match fs::remove_file(&journal_path) {
                Ok(()) => Ok(reply_from_readback(
                    "verified", "readback", false, backup_id, readback,
                )),
                Err(error) => Ok(ApplyReply {
                    status: "needs_recovery",
                    phase: "journal_cleanup",
                    needs_recovery: true,
                    manual_bootloader_confirmation_required: false,
                    bootloader_candidates,
                    backup_id,
                    firmware_version: Some(readback.info.firmware_version),
                    source_sha: Some(readback.info.source_sha),
                    device_id: Some(readback.info.device_id),
                    settings_preserved: readback.settings_preserved,
                    message: format!(
                        "Firmware and persistent settings read back successfully, but the recovery marker could not be cleared: {error}"
                    ),
                }),
            },
            Err(message) => Ok(ApplyReply {
                status: "needs_recovery",
                phase: "readback",
                needs_recovery: true,
                manual_bootloader_confirmation_required: false,
                bootloader_candidates,
                backup_id,
                firmware_version: None,
                source_sha: None,
                device_id: None,
                settings_preserved: None,
                message: format!(
                    "Firmware may have been written, but the required post-update identity/settings/version read-back did not pass: {message}"
                ),
            }),
        }
    }

    /// A recovery attempt only performs bounded read-back. It never reflashes.
    pub fn recover(&mut self) -> Result<RecoveryReply, String> {
        let path = self.pending_journal_path();
        if !path.exists() {
            return Ok(RecoveryReply {
                status: "none",
                needs_recovery: false,
                backup_id: None,
                firmware_version: None,
                source_sha: None,
                device_id: None,
                settings_preserved: None,
                message: "There is no incomplete firmware update to recover.".into(),
            });
        }
        let bytes = read_bounded(&path, MAX_JOURNAL_BYTES as usize)?;
        let journal: RecoveryJournal = serde_json::from_slice(&bytes).map_err(|error| {
            format!("The recovery journal is invalid; keep it and contact support: {error}")
        })?;
        validate_journal(&journal)?;
        let write_may_have_started = journal_write_may_have_started(&journal);
        let result = if write_may_have_started {
            self.verify_after_flash(&journal.binding, &journal.before, &journal.manifest)
        } else {
            self.verify_original_before_programmer(&journal.binding, &journal.before)
        };
        match result {
            Ok(readback) => match fs::remove_file(&path) {
                Ok(()) => Ok(RecoveryReply {
                    status: if write_may_have_started {
                        "verified"
                    } else {
                        "aborted_no_change"
                    },
                    needs_recovery: false,
                    backup_id: Some(journal.backup_id),
                    firmware_version: Some(readback.info.firmware_version),
                    source_sha: Some(readback.info.source_sha),
                    device_id: Some(readback.info.device_id),
                    settings_preserved: readback.settings_preserved,
                    message: if write_may_have_started {
                        "Recovery read-back verified the target firmware, identity, and available persistent settings. The incomplete-update marker was cleared.".into()
                    } else {
                        "No programmer was launched. Recovery read-back exactly matched the original protocol 3 firmware, source, identity, schema, and persistent settings; the update was aborted without change and the marker was cleared.".into()
                    },
                }),
                Err(error) => Ok(RecoveryReply {
                    status: "needs_recovery",
                    needs_recovery: true,
                    backup_id: Some(journal.backup_id),
                    firmware_version: Some(readback.info.firmware_version),
                    source_sha: Some(readback.info.source_sha),
                    device_id: Some(readback.info.device_id),
                    settings_preserved: readback.settings_preserved,
                    message: format!(
                        "Read-back passed, but the recovery marker remains because it could not be cleared: {error}"
                    ),
                }),
            },
            Err(message) => Ok(RecoveryReply {
                status: "needs_recovery",
                needs_recovery: true,
                backup_id: Some(journal.backup_id),
                firmware_version: None,
                source_sha: None,
                device_id: None,
                settings_preserved: None,
                message: format!(
                    "Recovery read-back has not passed. Do not retry flashing or select another device: {message}"
                ),
            }),
        }
    }

    fn load_package(
        &self,
        selected: Option<&SelectedPackageInput>,
    ) -> Result<PreparedPackage, String> {
        if let Some(input) = selected {
            if input.manifest_json.len() > MAX_MANIFEST_BYTES {
                return Err("The selected firmware manifest exceeds the size limit.".into());
            }
            if input.firmware_hex.len() > MAX_HEX_BYTES {
                return Err("The selected Intel HEX image exceeds the size limit.".into());
            }
            return self.prepare_package_bytes(
                input.manifest_json.as_bytes(),
                input.firmware_hex.as_bytes(),
                "user_selected_unverified",
                true,
            );
        }

        let bundle = self.resource_dir.join("distribution").join("firmware");
        let manifest_path = bundle.join("manifest.json");
        let image_path = bundle.join("firmware.hex");
        let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let image_bytes = read_bounded(&image_path, MAX_HEX_BYTES)?;
        let prepared = self.prepare_package_bytes(
            &manifest_bytes,
            &image_bytes,
            "bundled_provenance_matched",
            false,
        )?;
        if EXPECTED_FIRMWARE_MANIFEST_SHA256 != Some(prepared.manifest_sha256.as_str())
            || EXPECTED_FIRMWARE_IMAGE_SHA256 != Some(prepared.firmware_sha256.as_str())
            || EXPECTED_SOURCE_SHA != Some(prepared.manifest.source_sha.as_str())
        {
            return Err("The bundled package is not bound to this application build. Use a locally selected package only after reviewing its source and checksum.".into());
        }
        Ok(PreparedPackage {
            firmware_hex_path: image_path,
            ..prepared
        })
    }

    fn prepare_package_bytes(
        &self,
        manifest_bytes: &[u8],
        image_bytes: &[u8],
        source_trust: &'static str,
        persist_local: bool,
    ) -> Result<PreparedPackage, String> {
        if manifest_bytes.len() > MAX_MANIFEST_BYTES || image_bytes.len() > MAX_HEX_BYTES {
            return Err("The firmware package exceeds the supported size limits.".into());
        }
        let manifest: FirmwarePackageManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|error| format!("The firmware manifest is invalid: {error}"))?;
        let image_text = std::str::from_utf8(image_bytes)
            .map_err(|_| "The firmware image must be UTF-8 Intel HEX text.".to_string())?;
        validate_manifest_and_hex(&manifest, image_bytes.len() as u64, image_text)?;
        let manifest_sha256 = sha256(manifest_bytes);
        let firmware_sha256 = sha256(image_bytes);
        if firmware_sha256 != manifest.image_sha256 {
            return Err("The firmware image SHA-256 does not match the manifest.".into());
        }

        let firmware_hex_path = if persist_local {
            let package_id = sha256(format!("{manifest_sha256}:{firmware_sha256}").as_bytes());
            let directory = self
                .app_config_dir
                .join(MANAGER_UPDATE_DIR)
                .join("packages")
                .join(package_id);
            fs::create_dir_all(&directory).map_err(|error| {
                format!("Could not create the package staging directory: {error}")
            })?;
            let image_path = directory.join("firmware.hex");
            write_immutable_file(&image_path, image_bytes)?;
            let manifest_path = directory.join("manifest.json");
            write_immutable_file(&manifest_path, manifest_bytes)?;
            image_path
        } else {
            self.resource_dir
                .join("distribution")
                .join("firmware")
                .join("firmware.hex")
        };

        Ok(PreparedPackage {
            manifest,
            manifest_sha256,
            firmware_sha256,
            firmware_hex_path,
            source_trust,
        })
    }

    fn pending_journal_path(&self) -> PathBuf {
        self.app_config_dir
            .join(MANAGER_UPDATE_DIR)
            .join(PENDING_JOURNAL)
    }

    fn write_pending_journal(&self, journal: &RecoveryJournal) -> Result<PathBuf, String> {
        let directory = self.app_config_dir.join(MANAGER_UPDATE_DIR);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("Could not create the recovery directory: {error}"))?;
        let path = directory.join(PENDING_JOURNAL);
        let bytes = serde_json::to_vec_pretty(journal)
            .map_err(|error| format!("Could not encode the recovery journal: {error}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("Could not safely create the recovery marker: {error}"))?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not persist the recovery marker: {error}"))?;
        Ok(path)
    }

    fn persist_programmer_start(&self, plan: &PendingPlan) -> Result<(), String> {
        let path = self.pending_journal_path();
        let bytes = read_bounded(&path, MAX_JOURNAL_BYTES as usize)?;
        let mut journal: RecoveryJournal = serde_json::from_slice(&bytes).map_err(|error| {
            format!("The recovery marker is invalid before programmer start: {error}")
        })?;
        validate_journal(&journal)?;
        if journal.schema != 3
            || journal.write_may_have_started
            || journal.plan_id != plan.plan_id
            || journal.target != plan.target
            || journal.binding != plan.binding
            || journal.before != plan.before
            || journal.manifest != plan.package.manifest
            || journal.manifest_sha256 != plan.package.manifest_sha256
            || journal.firmware_sha256 != plan.package.firmware_sha256
            || journal.backup_id != plan.backup_id
            || journal.source_trust != plan.package.source_trust
        {
            return Err("The recovery marker no longer matches the prepared update or is already in the programmer stage.".into());
        }
        journal.write_may_have_started = true;
        self.replace_pending_journal(&journal)
    }

    fn replace_pending_journal(&self, journal: &RecoveryJournal) -> Result<(), String> {
        let directory = self.app_config_dir.join(MANAGER_UPDATE_DIR);
        let path = directory.join(PENDING_JOURNAL);
        let bytes = serde_json::to_vec_pretty(journal).map_err(|error| {
            format!("Could not encode the programmer-start recovery marker: {error}")
        })?;
        let temporary_path = directory.join(format!("{PENDING_JOURNAL}.{}.tmp", new_id("stage")));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|error| {
                format!("Could not safely stage the programmer-start marker: {error}")
            })?;
        if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&temporary_path);
            return Err(format!(
                "Could not persist the programmer-start marker: {error}"
            ));
        }
        drop(file);
        if let Err(error) = fs::rename(&temporary_path, &path) {
            let _ = fs::remove_file(&temporary_path);
            return Err(format!(
                "Could not atomically replace the recovery marker before programming: {error}"
            ));
        }
        Ok(())
    }

    fn verify_after_flash(
        &mut self,
        binding: &TargetBinding,
        before: &DeviceObservation,
        manifest: &FirmwarePackageManifest,
    ) -> Result<VerifiedReadback, String> {
        let started = Instant::now();
        let mut last_error = "The application port has not returned.".to_string();
        while started.elapsed() < POST_FLASH_TIMEOUT {
            let ports = self.backend.ports()?;
            match returning_application_target(&ports, binding) {
                Ok(target) => match self.backend.inspect(&target) {
                    Ok(DeviceObservation::Protocol3 { info }) => {
                        if let Err(error) = validate_info3(&info) {
                            last_error = error;
                        } else {
                            return verify_readback(info, before, manifest);
                        }
                    }
                    Ok(_) => {
                        last_error =
                            "The returning port did not provide protocol 3 read-back.".into()
                    }
                    Err(error) => last_error = error,
                },
                Err(error) => last_error = error,
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(last_error)
    }

    fn verify_original_before_programmer(
        &mut self,
        binding: &TargetBinding,
        before: &DeviceObservation,
    ) -> Result<VerifiedReadback, String> {
        let DeviceObservation::Protocol3 { info: expected } = before else {
            return Err("The original protocol 3 firmware, source, identity, schema, or settings were unavailable before this initial install; unchanged-state recovery cannot be claimed.".into());
        };
        let started = Instant::now();
        let mut last_error = "The original application port has not returned.".to_string();
        while started.elapsed() < POST_FLASH_TIMEOUT {
            let ports = self.backend.ports()?;
            match returning_application_target(&ports, binding) {
                Ok(target) => {
                    match self.backend.inspect(&target) {
                        Ok(DeviceObservation::Protocol3 { info }) => {
                            if let Err(error) = validate_info3(&info) {
                                last_error = error;
                            } else if info != *expected {
                                return Err("The recovery read-back differs from the exact original firmware, source, identity, protocol schema, or persistent settings snapshot.".into());
                            } else {
                                return Ok(VerifiedReadback {
                                    info,
                                    settings_preserved: Some(true),
                                });
                            }
                        }
                        Ok(_) => last_error =
                            "The returning port did not provide the original protocol 3 read-back."
                                .into(),
                        Err(error) => last_error = error,
                    }
                }
                Err(error) => last_error = error,
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(last_error)
    }
}

struct SystemBackend;

#[cfg(windows)]
fn windows_location_paths_by_port() -> Result<BTreeMap<String, String>, String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_PRESENT, SP_DEVINFO_DATA, SPDRP_FRIENDLYNAME, SPDRP_LOCATION_PATHS,
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
    };
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS, GetLastError};

    const MAX_DEVICES: u32 = 128;
    const MAX_PROPERTY_BYTES: u32 = 32 * 1024;
    const REG_SZ: u32 = 1;
    const REG_MULTI_SZ: u32 = 7;
    let ports_class = windows_sys::core::GUID::from_u128(0x4d36_e978_e325_11ce_bfc1_0800_2be1_0318);
    let set = unsafe {
        SetupDiGetClassDevsW(
            &ports_class,
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_PRESENT,
        )
    };
    if set == -1 {
        return Err("Windows SetupAPI could not enumerate present COM devices.".into());
    }
    struct DeviceSet(isize);
    impl Drop for DeviceSet {
        fn drop(&mut self) {
            unsafe {
                SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }
    let _set_guard = DeviceSet(set);
    let mut result = BTreeMap::new();
    for index in 0..MAX_DEVICES {
        let mut data: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        let enumerated = unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) };
        if enumerated == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_ITEMS {
                return Ok(result);
            }
            return Err(format!(
                "Windows SetupAPI could not enumerate COM device {index}: {error}"
            ));
        }
        let Some((friendly_type, friendly)) =
            read_setup_property(set, &data, SPDRP_FRIENDLYNAME, MAX_PROPERTY_BYTES)?
        else {
            continue;
        };
        if friendly_type != REG_SZ && friendly_type != REG_MULTI_SZ {
            continue;
        }
        let friendly = decode_windows_string(&friendly)?;
        let Some(port_name) = friendly
            .split_whitespace()
            .last()
            .and_then(parse_friendly_com_name)
        else {
            continue;
        };
        let location_path =
            read_setup_property(set, &data, SPDRP_LOCATION_PATHS, MAX_PROPERTY_BYTES)?
                .and_then(|(property_type, value)| {
                    if property_type != REG_MULTI_SZ {
                        return None;
                    }
                    let paths = decode_windows_multisz(&value).ok()?;
                    if paths.len() != 1 {
                        return None;
                    }
                    Some(paths.into_iter().next().unwrap_or_default())
                })
                .filter(|path| !path.is_empty());
        if let Some(path) = location_path.as_deref()
            && (path.len() > 1024
                || !path.is_ascii()
                || path.bytes().any(|byte| byte.is_ascii_control()))
        {
            return Err("Windows returned an invalid USB location path for a COM device.".into());
        }
        if let Some(path) = location_path
            && result.insert(port_name.clone(), path).is_some()
        {
            return Err(format!(
                "Windows returned more than one PnP location for {port_name}."
            ));
        }
    }
    Err("Windows SetupAPI exceeded the 128-device topology limit.".into())
}

#[cfg(windows)]
fn read_setup_property(
    set: isize,
    data: &windows_sys::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    property: u32,
    maximum: u32,
) -> Result<Option<(u32, Vec<u16>)>, String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::SetupDiGetDeviceRegistryPropertyW;
    let mut property_type = 0u32;
    let mut required = 0u32;
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            property,
            &mut property_type,
            std::ptr::null_mut(),
            0,
            &mut required,
        );
    }
    if required == 0 {
        return Ok(None);
    }
    if required > maximum || !required.is_multiple_of(2) {
        return Err("A Windows device property exceeds the supported UTF-16 size.".into());
    }
    let mut value = vec![0u16; required as usize / 2];
    let read = unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set,
            data,
            property,
            &mut property_type,
            value.as_mut_ptr().cast::<u8>(),
            required,
            &mut required,
        )
    };
    if read == 0 {
        return Err("Windows could not read a COM device property after sizing it.".into());
    }
    value.truncate(required as usize / 2);
    Ok(Some((property_type, value)))
}

#[cfg(windows)]
fn decode_windows_string(value: &[u16]) -> Result<String, String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16(&value[..end])
        .map_err(|_| "Windows returned invalid UTF-16 device metadata.".into())
}

#[cfg(windows)]
fn decode_windows_multisz(value: &[u16]) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut start = 0;
    for end in 0..=value.len() {
        if end == value.len() || value[end] == 0 {
            if end > start {
                let entry = String::from_utf16(&value[start..end]).map_err(|_| {
                    "Windows returned invalid UTF-16 USB location metadata.".to_string()
                })?;
                result.push(entry);
            }
            start = end + 1;
        }
    }
    Ok(result)
}

#[cfg(windows)]
fn parse_friendly_com_name(value: &str) -> Option<String> {
    let name = value.strip_prefix('(')?.strip_suffix(')')?;
    let suffix = name.strip_prefix("COM")?;
    if suffix.is_empty()
        || !suffix.bytes().all(|byte| byte.is_ascii_digit())
        || suffix.parse::<u16>().ok()? == 0
    {
        return None;
    }
    Some(name.to_owned())
}

#[cfg(not(windows))]
fn windows_location_paths_by_port() -> Result<BTreeMap<String, String>, String> {
    Ok(BTreeMap::new())
}

impl Backend for SystemBackend {
    fn ports(&mut self) -> Result<Vec<PortRecord>, String> {
        let summaries = available_port_summaries().map_err(|error| error.to_string())?;
        if summaries.len() > 128 {
            return Err("The USB topology exceeds the supported 128-port limit.".into());
        }
        let location_paths = windows_location_paths_by_port()?;
        Ok(summaries
            .into_iter()
            .map(PortView::from)
            .map(|view| {
                let location_path = location_paths.get(&view.target.name).cloned();
                PortRecord {
                    key: view.target,
                    location_path,
                }
            })
            .collect())
    }

    fn inspect(&mut self, target: &PortKey) -> Result<DeviceObservation, String> {
        validate_target_key(target)?;
        let mut transport = SerialPortTransport::open(&target.name)
            .map_err(|error| format!("Could not open the selected device port: {error}"))?;
        let request = next_request_id();
        transport
            .discard_input()
            .map_err(|error| format!("Could not clear stale serial input: {error}"))?;
        transport
            .write_line(&format!("info3,{request}\n"))
            .map_err(|error| format!("Could not request protocol 3 device information: {error}"))?;

        match read_info3(&mut transport, request) {
            Ok(Some(info)) => return Ok(DeviceObservation::Protocol3 { info }),
            Ok(None) => {}
            Err(error) => return Err(error),
        }

        transport
            .discard_input()
            .map_err(|error| format!("Could not clear stale serial input: {error}"))?;
        transport
            .write_line("info\n")
            .map_err(|error| format!("Could not request legacy device information: {error}"))?;
        read_legacy_info(&mut transport)
    }

    fn reset_to_bootloader(&mut self, target: &PortKey) -> Result<(), FlashError> {
        #[cfg(windows)]
        {
            let port = serialport::new(&target.name, 1200)
                .timeout(Duration::from_millis(500))
                .dtr_on_open(true)
                .open()
                .map_err(|error| FlashError {
                    writes_may_have_started: false,
                    message: format!("Could not open the selected application port for the 1200-bps bootloader touch: {error}"),
                })?;
            drop(port);
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = target;
            Err(FlashError {
                writes_may_have_started: false,
                message: "The firmware updater requires the Windows native USB reset path.".into(),
            })
        }
    }

    fn flash(
        &mut self,
        target: &PortKey,
        image_path: &Path,
        runtime: &ProgrammerRuntime,
    ) -> Result<(), FlashError> {
        run_avrdude(target, image_path, runtime)
    }
}

fn read_info3(
    transport: &mut impl LineTransport,
    request: u32,
) -> Result<Option<DeviceInfo3>, String> {
    let deadline = Instant::now() + INFO_TIMEOUT;
    for _ in 0..MAX_DEVICE_LINES {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(None);
        };
        if remaining.is_zero() {
            return Ok(None);
        }
        let line = match transport.read_line(remaining) {
            Ok(line) => line,
            Err(TransportError::Timeout) => return Ok(None),
            Err(error) => return Err(format!("Protocol 3 read-back failed: {error}")),
        };
        match selfrionette_core::protocol_v3::parse_line(&line)
            .map_err(|error| format!("Protocol 3 frame is invalid: {error}"))?
        {
            selfrionette_core::protocol_v3::Frame::Info {
                request: actual,
                info,
            } if actual == request => {
                let info = DeviceInfo3 {
                    firmware_version: info.firmware_version,
                    source_sha: info.source_revision,
                    board: info.board,
                    mcu: info.mcu,
                    clock_hz: info.clock_hz,
                    device_id: info.identity.to_string(),
                    generation: info.generation,
                    assembly: info.assembly_revision,
                    known: info.known_mask,
                    negative: info.negative_mask,
                    tare_epoch: info.tare_epoch,
                    tare_valid: info.tare_valid,
                };
                validate_info3(&info)?;
                return Ok(Some(info));
            }
            selfrionette_core::protocol_v3::Frame::Error {
                request: actual, ..
            } if actual == request => {
                return Err("The firmware rejected the protocol 3 information request.".into());
            }
            _ => {}
        }
    }
    Ok(None)
}

fn read_legacy_info(transport: &mut impl LineTransport) -> Result<DeviceObservation, String> {
    let deadline = Instant::now() + INFO_TIMEOUT;
    for _ in 0..MAX_DEVICE_LINES {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(DeviceObservation::Unidentified);
        };
        if remaining.is_zero() {
            return Ok(DeviceObservation::Unidentified);
        }
        let line = match transport.read_line(remaining) {
            Ok(line) => line,
            Err(TransportError::Timeout) => return Ok(DeviceObservation::Unidentified),
            Err(error) => return Err(format!("Legacy information read-back failed: {error}")),
        };
        if line.len() > 511 || !line.is_ascii() {
            return Err("The device returned an invalid or oversized protocol line.".into());
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.first() != Some(&"device") {
            continue;
        }
        if fields.len() != 5 || fields[1] != "2" || fields[4] != "7" {
            return Err("The legacy device information frame is malformed or incompatible.".into());
        }
        validate_version(fields[2])?;
        validate_device_id(fields[3])?;
        return Ok(DeviceObservation::Legacy {
            info: LegacyDeviceInfo {
                firmware_version: fields[2].into(),
                device_id: fields[3].into(),
            },
        });
    }
    Ok(DeviceObservation::Unidentified)
}

fn validate_target_key(target: &PortKey) -> Result<(), String> {
    if target.name.len() > 16
        || target.name.is_empty()
        || !target.name.starts_with("COM")
        || !target.name[3..].bytes().all(|byte| byte.is_ascii_digit())
        || target.name[3..]
            .parse::<u16>()
            .map_or(true, |number| number == 0)
    {
        return Err("Firmware updates require an explicitly selected Windows COM port.".into());
    }
    if target.kind != "usb" {
        return Err("The selected target is not an enumerated USB serial port.".into());
    }
    let category = crate::discovery::classify(&target.kind, target.vid, target.pid).0;
    if category != "candidate" {
        return Err(
            "The selected USB VID/PID is not a supported Pro Micro / ATmega32U4 candidate.".into(),
        );
    }
    for value in [
        target.serial_number.as_deref(),
        target.product.as_deref(),
        target.manufacturer.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > 256 || value.contains(['\r', '\n', '\0']) {
            return Err("The selected USB descriptor contains invalid text.".into());
        }
    }
    Ok(())
}

fn port_category(port: &PortKey) -> &'static str {
    crate::discovery::classify(&port.kind, port.vid, port.pid).0
}

fn validate_topology(
    ports: &[PortRecord],
    target: &PortKey,
) -> Result<(TopologySummary, TargetBinding), String> {
    if ports.len() > 128 {
        return Err("The USB topology exceeds the supported 128-port limit.".into());
    }
    let selected: Vec<&PortRecord> = ports.iter().filter(|port| port.key == *target).collect();
    let selected_port_matches = selected.len();
    if selected_port_matches != 1 {
        return Err(
            "The selected USB descriptor is no longer unique. Rescan and choose the target again."
                .into(),
        );
    }
    let other_candidate_count = ports
        .iter()
        .filter(|port| port_category(&port.key) == "candidate")
        .filter(|port| port.key != *target)
        .count();
    let record = selected[0];
    let location_path = record.location_path.clone();
    if let Some(location_path) = location_path.as_deref() {
        let location_matches = ports
            .iter()
            .filter(|port| port.location_path.as_deref() == Some(location_path))
            .count();
        if location_matches != 1 {
            return Err("The selected USB location path is duplicated across active ports; the update target is ambiguous.".into());
        }
    }
    let serial_number = target
        .serial_number
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if let Some(serial) = serial_number.as_deref() {
        let serial_matches = ports
            .iter()
            .filter(|port| {
                port.key.kind == "usb" && port.key.serial_number.as_deref() == Some(serial)
            })
            .count();
        if serial_matches != 1 {
            return Err("The USB serial descriptor is duplicated across ports; the update target is ambiguous.".into());
        }
    }
    let binding = TargetBinding {
        location_path,
        serial_number: serial_number.clone(),
    };
    let status = if binding.location_path.is_some() {
        "unique_usb_location_path"
    } else if binding.serial_number.is_some() {
        "unique_usb_serial"
    } else {
        "selected_com_requires_operator_confirmation"
    };
    Ok((
        TopologySummary {
            selected_port_matches,
            other_candidate_count,
            stable_usb_serial_available: binding.serial_number.is_some(),
            location_path_available: binding.location_path.is_some(),
            status,
        },
        binding,
    ))
}

#[derive(Debug)]
enum BootloaderSelection {
    Ready(PortKey),
    NeedsManualConfirmation(Vec<PortKey>),
}

fn wait_for_bootloader(
    backend: &mut dyn Backend,
    original_target: &PortKey,
    binding: &TargetBinding,
) -> Result<Vec<PortRecord>, String> {
    const RESET_TIMEOUT: Duration = Duration::from_secs(12);
    let started = Instant::now();
    let mut last_candidates = Vec::new();
    while started.elapsed() < RESET_TIMEOUT {
        let ports = backend.ports()?;
        let application_still_present = ports
            .iter()
            .any(|port| port.key == *original_target && port_category(&port.key) == "candidate");
        let bootloaders: Vec<&PortRecord> = ports
            .iter()
            .filter(|port| port_category(&port.key) == "bootloader")
            .collect();
        last_candidates = bootloaders.iter().map(|port| port.key.clone()).collect();
        if !application_still_present && !bootloaders.is_empty() {
            if let Some(path) = binding.location_path.as_deref() {
                let matches = bootloaders
                    .iter()
                    .filter(|port| port.location_path.as_deref() == Some(path))
                    .count();
                if matches > 1 {
                    return Err(
                        "Several bootloader ports share the selected USB location path.".into(),
                    );
                }
            }
            if let Some(serial) = binding.serial_number.as_deref() {
                let matches = bootloaders
                    .iter()
                    .filter(|port| port.key.serial_number.as_deref() == Some(serial))
                    .count();
                if matches > 1 {
                    return Err(
                        "Several bootloader ports share the selected USB serial descriptor.".into(),
                    );
                }
            }
            return Ok(ports);
        }
        thread::sleep(Duration::from_millis(200));
    }
    if last_candidates.is_empty() {
        Err("No bootloader port re-enumerated after the 1200-bps touch.".into())
    } else {
        Err(
            "The application COM port did not disappear before the bootloader candidate appeared."
                .into(),
        )
    }
}

fn resolve_bootloader_target(
    ports: &[PortRecord],
    _original_target: &PortKey,
    binding: &TargetBinding,
    requested: Option<&PortKey>,
    manual_confirmation: bool,
    previously_requested: bool,
) -> Result<BootloaderSelection, String> {
    let candidates: Vec<&PortRecord> = ports
        .iter()
        .filter(|port| port_category(&port.key) == "bootloader")
        .collect();
    if candidates.is_empty() {
        return Err("No bootloader port is present after reset.".into());
    }

    let mut stable_matches = Vec::new();
    let mut descriptor_conflicts = Vec::new();
    for candidate in &candidates {
        let location_match = binding
            .location_path
            .as_deref()
            .zip(candidate.location_path.as_deref())
            .is_some_and(|(expected, observed)| expected == observed);
        let serial_match = binding
            .serial_number
            .as_deref()
            .zip(candidate.key.serial_number.as_deref())
            .is_some_and(|(expected, observed)| expected == observed);
        let location_conflict = binding
            .location_path
            .as_deref()
            .zip(candidate.location_path.as_deref())
            .is_some_and(|(expected, observed)| expected != observed);
        let serial_conflict = binding
            .serial_number
            .as_deref()
            .zip(candidate.key.serial_number.as_deref())
            .is_some_and(|(expected, observed)| expected != observed);
        if location_conflict || serial_conflict {
            descriptor_conflicts.push(candidate.key.clone());
        } else if location_match || serial_match {
            stable_matches.push(candidate.key.clone());
        }
    }
    match stable_matches.as_slice() {
        [target] => {
            if requested.is_some() || manual_confirmation {
                return Err("The current topology already binds the bootloader to the selected physical USB port; manual target substitution is prohibited.".into());
            }
            return Ok(BootloaderSelection::Ready(target.clone()));
        }
        [] => {}
        _ => return Err("More than one bootloader port matches the selected USB topology.".into()),
    }

    if candidates.len() != 1 {
        return Err("The bootloader topology is ambiguous without a matching location path or serial number.".into());
    }
    if !descriptor_conflicts.is_empty() {
        return Err("A known USB location path or serial descriptor contradicts the selected device. Manual confirmation cannot override conflicting topology.".into());
    }
    let sole_candidate = &candidates[0].key;
    let has_missing_descriptor = binding.location_path.is_none()
        || candidates[0].location_path.is_none()
        || binding.serial_number.is_none()
        || sole_candidate.serial_number.is_none();
    if !has_missing_descriptor {
        return Err("The unique bootloader candidate has complete topology descriptors but does not match the selected device; manual confirmation cannot override conflicting topology.".into());
    }
    if !previously_requested {
        if requested.is_some() || manual_confirmation {
            return Err("The manual bootloader target can only be supplied after the service has requested confirmation.".into());
        }
        return Ok(BootloaderSelection::NeedsManualConfirmation(vec![
            sole_candidate.clone(),
        ]));
    }
    if !manual_confirmation || requested != Some(sole_candidate) {
        return Err(
            "The unique bootloader port must be selected and confirmed explicitly before flashing."
                .into(),
        );
    }
    Ok(BootloaderSelection::Ready(sole_candidate.clone()))
}

fn returning_application_target(
    ports: &[PortRecord],
    binding: &TargetBinding,
) -> Result<PortKey, String> {
    let candidates: Vec<&PortRecord> = ports
        .iter()
        .filter(|port| port_category(&port.key) == "candidate")
        .collect();
    if candidates.is_empty() {
        return Err("No application-mode USB port has returned after flashing.".into());
    }
    let matches: Vec<&PortRecord> = if let Some(path) = binding.location_path.as_deref() {
        candidates
            .iter()
            .copied()
            .filter(|port| port.location_path.as_deref() == Some(path))
            .collect()
    } else if let Some(serial) = binding.serial_number.as_deref() {
        candidates
            .iter()
            .copied()
            .filter(|port| port.key.serial_number.as_deref() == Some(serial))
            .collect()
    } else {
        candidates.clone()
    };
    match matches.as_slice() {
        [target] => Ok(target.key.clone()),
        [] => Err("The application-mode port does not match the selected USB location or serial binding.".into()),
        _ => Err("More than one application-mode port matches the selected target; recovery remains required.".into()),
    }
}

fn validate_observation(observation: &DeviceObservation) -> Result<(), String> {
    match observation {
        DeviceObservation::Protocol3 { info } => validate_info3(info),
        DeviceObservation::Legacy { info } => {
            validate_version(&info.firmware_version)?;
            validate_device_id(&info.device_id)
        }
        DeviceObservation::Unidentified => Ok(()),
    }
}

fn validate_board_if_reported(observation: &DeviceObservation) -> Result<(), String> {
    if let DeviceObservation::Protocol3 { info } = observation
        && (info.board != BOARD || info.mcu != MCU || info.clock_hz != CLOCK_HZ)
    {
        return Err(
            "The selected device reported a board, MCU, or clock outside the supported target."
                .into(),
        );
    }
    Ok(())
}

fn validate_info3(info: &DeviceInfo3) -> Result<(), String> {
    validate_version(&info.firmware_version)?;
    validate_sha40(&info.source_sha, "firmware source SHA")?;
    validate_device_id(&info.device_id)?;
    if info.board != BOARD || info.mcu != MCU || info.clock_hz != CLOCK_HZ {
        return Err(
            "The device information does not match the supported board, MCU, and clock.".into(),
        );
    }
    if info.known & !0x7f != 0 || info.negative & !info.known != 0 {
        return Err("The device settings masks are invalid.".into());
    }
    if info.generation == 0 {
        if info.assembly != 0 || info.known != 0 || info.negative != 0 {
            return Err(
                "The device reports an inconsistent unconfigured settings generation.".into(),
            );
        }
    } else if info.assembly == 0 || info.known == 0 || info.device_id == "unprovisioned" {
        return Err("The device reports a settings generation without a complete identity and settings record.".into());
    }
    Ok(())
}

fn validate_manifest_and_hex(
    manifest: &FirmwarePackageManifest,
    image_size: u64,
    image_text: &str,
) -> Result<IntelHexReport, String> {
    validate_manifest(manifest, image_size)?;
    let parsed = parse_intel_hex(image_text)?;
    if parsed.bytes.is_empty() {
        return Err("The firmware image contains no application flash data.".into());
    }
    Ok(parsed)
}

fn validate_manifest(manifest: &FirmwarePackageManifest, image_size: u64) -> Result<(), String> {
    if manifest.schema != 1
        || manifest.release_version != APP_RELEASE_VERSION
        || manifest.application != APPLICATION
        || manifest.board != BOARD
        || manifest.mcu != MCU
        || manifest.clock_hz != CLOCK_HZ
        || manifest.protocol_schema != PROTOCOL_SCHEMA
        || manifest.settings_schema != SETTINGS_SCHEMA
        || manifest.flash_start != APP_FLASH_START
        || manifest.flash_end_exclusive != APP_FLASH_END_EXCLUSIVE
        || manifest.image_file != "firmware.hex"
    {
        return Err("The firmware manifest targets an unsupported application, schema, version, board, MCU, or flash range.".into());
    }
    validate_version(&manifest.firmware_version)?;
    validate_sha40(&manifest.source_sha, "firmware source SHA")?;
    validate_sha256(&manifest.image_sha256, "firmware image SHA-256")?;
    if manifest.image_size != image_size {
        return Err("The firmware image byte length does not match the manifest.".into());
    }
    if image_size == 0 || image_size > MAX_HEX_BYTES as u64 {
        return Err("The firmware image size is outside the supported range.".into());
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct IntelHexReport {
    bytes: BTreeMap<u32, u8>,
    max_address_exclusive: u32,
}

fn parse_intel_hex(input: &str) -> Result<IntelHexReport, String> {
    if input.len() > MAX_HEX_BYTES || !input.is_ascii() {
        return Err("The Intel HEX image is non-ASCII or exceeds the size limit.".into());
    }
    let mut bytes = BTreeMap::new();
    let mut base = 0u32;
    let mut eof_seen = false;
    let mut start_address_seen = false;
    let mut max_address_exclusive = 0u32;
    let mut record_count = 0usize;
    for (line_number, line) in input.lines().enumerate() {
        record_count += 1;
        if record_count > MAX_HEX_RECORDS {
            return Err("The Intel HEX image contains too many records.".into());
        }
        if eof_seen {
            return Err("The Intel HEX image contains data after EOF.".into());
        }
        if !line.starts_with(':') || line.len() < 11 || (line.len() - 1) % 2 != 0 {
            return Err(format!(
                "Intel HEX record {} has invalid framing.",
                line_number + 1
            ));
        }
        let raw = decode_hex(&line.as_bytes()[1..]).map_err(|_| {
            format!(
                "Intel HEX record {} contains invalid hexadecimal digits.",
                line_number + 1
            )
        })?;
        let count = raw[0] as usize;
        if raw.len() != count + 5 {
            return Err(format!(
                "Intel HEX record {} has an invalid byte count.",
                line_number + 1
            ));
        }
        if raw.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) != 0 {
            return Err(format!(
                "Intel HEX record {} has a bad checksum.",
                line_number + 1
            ));
        }
        let address = u16::from_be_bytes([raw[1], raw[2]]) as u32;
        let record_type = raw[3];
        let data = &raw[4..4 + count];
        match record_type {
            0x00 => {
                if count == 0 {
                    return Err(format!(
                        "Intel HEX record {} is an empty data record.",
                        line_number + 1
                    ));
                }
                let start = base
                    .checked_add(address)
                    .ok_or_else(|| "Intel HEX address overflowed.".to_string())?;
                let end = start
                    .checked_add(count as u32)
                    .ok_or_else(|| "Intel HEX address overflowed.".to_string())?;
                if !(APP_FLASH_START..APP_FLASH_END_EXCLUSIVE).contains(&start)
                    || end > APP_FLASH_END_EXCLUSIVE
                {
                    return Err(format!(
                        "Intel HEX record {} reaches outside application flash and may target the bootloader or EEPROM.",
                        line_number + 1
                    ));
                }
                for (offset, value) in data.iter().enumerate() {
                    let location = start + offset as u32;
                    if bytes.insert(location, *value).is_some() {
                        return Err(format!(
                            "Intel HEX record {} overlaps an earlier address.",
                            line_number + 1
                        ));
                    }
                }
                max_address_exclusive = max_address_exclusive.max(end);
            }
            0x01 => {
                if count != 0 || address != 0 {
                    return Err("Intel HEX EOF record must have zero address and no data.".into());
                }
                eof_seen = true;
            }
            0x02 => {
                if count != 2 || address != 0 {
                    return Err("Intel HEX extended segment address record is malformed.".into());
                }
                let segment = u16::from_be_bytes([data[0], data[1]]) as u32;
                base = segment << 4;
                if base >= APP_FLASH_END_EXCLUSIVE {
                    return Err(
                        "Intel HEX extended segment address reaches outside application flash."
                            .into(),
                    );
                }
            }
            0x04 => {
                if count != 2 || address != 0 {
                    return Err("Intel HEX extended linear address record is malformed.".into());
                }
                let upper = u16::from_be_bytes([data[0], data[1]]) as u32;
                base = upper << 16;
                if base >= APP_FLASH_END_EXCLUSIVE {
                    return Err(
                        "Intel HEX extended linear address reaches outside application flash."
                            .into(),
                    );
                }
            }
            0x03 | 0x05 => {
                if count != 4 || address != 0 || start_address_seen {
                    return Err("Intel HEX start-address record is malformed or duplicated.".into());
                }
                let start = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                if start >= APP_FLASH_END_EXCLUSIVE {
                    return Err("Intel HEX start address reaches outside application flash.".into());
                }
                start_address_seen = true;
            }
            _ => {
                return Err(format!(
                    "Intel HEX record type 0x{record_type:02x} is unsupported."
                ));
            }
        }
    }
    if !eof_seen {
        return Err("Intel HEX image is missing its EOF record.".into());
    }
    Ok(IntelHexReport {
        bytes,
        max_address_exclusive,
    })
}

fn verify_readback(
    info: DeviceInfo3,
    before: &DeviceObservation,
    manifest: &FirmwarePackageManifest,
) -> Result<VerifiedReadback, String> {
    if info.firmware_version != manifest.firmware_version {
        return Err("The read-back firmware version does not match the selected package.".into());
    }
    if info.source_sha != manifest.source_sha {
        return Err(
            "The read-back firmware source SHA does not match the selected package.".into(),
        );
    }
    if info.board != manifest.board
        || info.mcu != manifest.mcu
        || info.clock_hz != manifest.clock_hz
    {
        return Err(
            "The read-back board, MCU, or clock does not match the selected package.".into(),
        );
    }

    let expected_id = match before {
        DeviceObservation::Protocol3 { info } => Some(info.device_id.as_str()),
        DeviceObservation::Legacy { info } => Some(info.device_id.as_str()),
        DeviceObservation::Unidentified => None,
    };
    if expected_id.is_some_and(|expected| expected != info.device_id) {
        return Err(
            "The post-update device identity differs from the pre-update read-back.".into(),
        );
    }
    let settings_preserved = match before {
        DeviceObservation::Protocol3 { info: prior } => {
            let preserved = prior.generation == info.generation
                && prior.assembly == info.assembly
                && prior.known == info.known
                && prior.negative == info.negative;
            if !preserved {
                return Err(
                    "The persistent settings generation or values changed during firmware update."
                        .into(),
                );
            }
            Some(true)
        }
        _ => None,
    };
    Ok(VerifiedReadback {
        info,
        settings_preserved,
    })
}

struct VerifiedReadback {
    info: DeviceInfo3,
    settings_preserved: Option<bool>,
}

fn reply_from_readback(
    status: &'static str,
    phase: &'static str,
    needs_recovery: bool,
    backup_id: String,
    readback: VerifiedReadback,
) -> ApplyReply {
    let message = if readback.settings_preserved == Some(true) {
        "Firmware version, source, identity, and persistent settings read-back all match the pre-update plan.".into()
    } else {
        "Firmware version, source, board, MCU, clock, and post-update identity/settings read-back passed. Previous identity/settings were unavailable before this initial installation, so preservation could not be compared.".into()
    };
    ApplyReply {
        status,
        phase,
        needs_recovery,
        manual_bootloader_confirmation_required: false,
        bootloader_candidates: Vec::new(),
        backup_id,
        firmware_version: Some(readback.info.firmware_version),
        source_sha: Some(readback.info.source_sha),
        device_id: Some(readback.info.device_id),
        settings_preserved: readback.settings_preserved,
        message,
    }
}

fn recovery_apply_reply(
    phase: &'static str,
    backup_id: String,
    bootloader_candidates: Vec<PortKey>,
    message: String,
) -> ApplyReply {
    ApplyReply {
        status: "needs_recovery",
        phase,
        needs_recovery: true,
        manual_bootloader_confirmation_required: false,
        bootloader_candidates,
        backup_id,
        firmware_version: None,
        source_sha: None,
        device_id: None,
        settings_preserved: None,
        message,
    }
}

fn make_backup_record(
    backup_id: &str,
    target: PortKey,
    observation: DeviceObservation,
    package: &PreparedPackage,
) -> BackupRecord {
    let settings_snapshot = match &observation {
        DeviceObservation::Protocol3 { info } => Some(SettingsSnapshot {
            generation: info.generation,
            assembly: info.assembly,
            known: info.known,
            negative: info.negative,
        }),
        _ => None,
    };
    let settings_backup_status = match &observation {
        DeviceObservation::Protocol3 { .. } => "protocol3_identity_and_settings",
        DeviceObservation::Legacy { .. } => "legacy_identity_only",
        DeviceObservation::Unidentified => "pre_update_identity_and_settings_unavailable",
    };
    BackupRecord {
        schema: 1,
        backup_id: backup_id.into(),
        captured_unix_seconds: unix_seconds(),
        target,
        observation,
        settings_snapshot,
        settings_backup_status,
        firmware_package_sha256: package.firmware_sha256.clone(),
        source_trust: package.source_trust,
    }
}

fn persist_backup(app_config_dir: &Path, record: &BackupRecord) -> Result<PathBuf, String> {
    let directory = app_config_dir.join(MANAGER_UPDATE_DIR).join("backups");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the update backup directory: {error}"))?;
    let path = directory.join(format!("{}.json", record.backup_id));
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| format!("Could not encode the pre-update snapshot: {error}"))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("Could not safely persist the pre-update snapshot: {error}"))?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(format!("Could not finish the pre-update snapshot: {error}"));
    }
    Ok(path)
}

fn write_immutable_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if path.exists() {
        let existing = read_bounded(path, bytes.len())?;
        if existing == bytes {
            return Ok(());
        }
        return Err("An existing immutable package file differs from the selected package.".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not safely stage the selected package: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!(
            "Could not finish staging the selected package: {error}"
        ));
    }
    Ok(())
}

fn validate_journal(journal: &RecoveryJournal) -> Result<(), String> {
    if !matches!(journal.schema, 2 | 3)
        || journal.plan_id.len() > 64
        || journal.backup_id.len() > 64
        || !matches!(
            journal.source_trust.as_str(),
            "user_selected_unverified" | "bundled_provenance_matched"
        )
    {
        return Err("The recovery journal has unsupported or invalid fields.".into());
    }
    validate_target_key(&journal.target)?;
    if let Some(path) = journal.binding.location_path.as_deref()
        && (path.is_empty()
            || path.len() > 1024
            || !path.is_ascii()
            || path.bytes().any(|byte| byte.is_ascii_control()))
    {
        return Err("The recovery journal USB location binding is invalid.".into());
    }
    if let Some(serial) = journal.binding.serial_number.as_deref()
        && (serial.is_empty() || serial.len() > 256 || serial.contains(['\r', '\n', '\0']))
    {
        return Err("The recovery journal USB serial binding is invalid.".into());
    }
    validate_observation(&journal.before)?;
    validate_manifest(&journal.manifest, journal.manifest.image_size)?;
    validate_sha256(&journal.manifest_sha256, "firmware manifest SHA-256")?;
    validate_sha256(&journal.firmware_sha256, "firmware image SHA-256")?;
    if journal.manifest.image_sha256 != journal.firmware_sha256 {
        return Err("The recovery journal package hashes do not agree.".into());
    }
    Ok(())
}

fn legacy_journal_may_have_written() -> bool {
    true
}

fn journal_write_may_have_started(journal: &RecoveryJournal) -> bool {
    // Schema 2 predates the durable stage marker, so its write state is unknown.
    journal.schema == 2 || journal.write_may_have_started
}

fn runtime_paths(resource_dir: &Path) -> ProgrammerRuntime {
    let directory = resource_dir.join("distribution").join("firmware-tools");
    ProgrammerRuntime {
        executable: directory.join("avrdude.exe"),
        config: directory.join("avrdude.conf"),
    }
}

fn verify_runtime_hashes(resource_dir: &Path, pins: &RuntimePins) -> Result<(), String> {
    let runtime = runtime_paths(resource_dir);
    let expected_exe = pins.executable_sha256.as_deref().ok_or_else(|| {
        "This application build has no pinned avrdude runtime provenance.".to_string()
    })?;
    let expected_conf = pins.config_sha256.as_deref().ok_or_else(|| {
        "This application build has no pinned avrdude configuration provenance.".to_string()
    })?;
    validate_sha256(expected_exe, "pinned avrdude executable SHA-256")?;
    validate_sha256(expected_conf, "pinned avrdude configuration SHA-256")?;
    if sha256(&read_bounded(&runtime.executable, 32 * 1024 * 1024)?) != expected_exe {
        return Err(
            "The packaged avrdude executable does not match the pinned runtime hash.".into(),
        );
    }
    if sha256(&read_bounded(&runtime.config, 4 * 1024 * 1024)?) != expected_conf {
        return Err(
            "The packaged avrdude configuration does not match the pinned runtime hash.".into(),
        );
    }
    Ok(())
}

fn run_avrdude(
    target: &PortKey,
    image_path: &Path,
    runtime: &ProgrammerRuntime,
) -> Result<(), FlashError> {
    if !Path::new(".").is_dir() || !image_path.is_file() {
        return Err(FlashError {
            writes_may_have_started: false,
            message: "The validated firmware image is missing.".into(),
        });
    }
    if !runtime.executable.is_file() || !runtime.config.is_file() {
        return Err(FlashError {
            writes_may_have_started: false,
            message: "The fixed packaged programmer or configuration is missing.".into(),
        });
    }
    let file_name = image_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name == "firmware.hex")
        .ok_or_else(|| FlashError {
            writes_may_have_started: false,
            message: "The staged image filename is invalid.".into(),
        })?;
    let port = target.name.as_str();
    let args = programmer_args(port, file_name, &runtime.config);
    let mut command = Command::new(&runtime.executable);
    command
        .args(&args)
        .current_dir(image_path.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_child_environment(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| FlashError {
        writes_may_have_started: false,
        message: format!("The pinned programmer could not start: {error}"),
    })?;
    let stdout = child.stdout.take().ok_or_else(|| FlashError {
        writes_may_have_started: true,
        message: "The programmer output pipe could not be opened.".into(),
    })?;
    let stderr = child.stderr.take().ok_or_else(|| FlashError {
        writes_may_have_started: true,
        message: "The programmer error pipe could not be opened.".into(),
    })?;
    let out_thread = thread::spawn(move || drain_bounded(stdout, MAX_PROGRAMMER_OUTPUT));
    let err_thread = thread::spawn(move || drain_bounded(stderr, MAX_PROGRAMMER_OUTPUT));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < PROGRAMMER_TIMEOUT => {
                thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return Err(FlashError {
                    writes_may_have_started: true,
                    message: format!("Could not observe the programmer process: {error}"),
                });
            }
        }
    };
    let stdout = out_thread
        .join()
        .unwrap_or_else(|_| Ok(Vec::new()))
        .unwrap_or_default();
    let stderr = err_thread
        .join()
        .unwrap_or_else(|_| Ok(Vec::new()))
        .unwrap_or_default();
    if status.is_none() {
        return Err(FlashError {
            writes_may_have_started: true,
            message: "The programmer exceeded the 120-second limit and was stopped.".into(),
        });
    }
    let status = status.expect("checked above");
    if !status.success() {
        let detail = bounded_output(&stderr, &stdout);
        return Err(FlashError {
            writes_may_have_started: true,
            message: if detail.is_empty() {
                format!(
                    "The programmer exited with status {}.",
                    status.code().unwrap_or(-1)
                )
            } else {
                format!(
                    "The programmer exited with status {}: {detail}",
                    status.code().unwrap_or(-1)
                )
            },
        });
    }
    Ok(())
}

fn programmer_args(port: &str, image_file: &str, config: &Path) -> Vec<std::ffi::OsString> {
    use std::ffi::OsString;
    vec![
        OsString::from("-N"),
        OsString::from("-C"),
        config.as_os_str().to_owned(),
        OsString::from("-p"),
        OsString::from(MCU),
        OsString::from("-c"),
        OsString::from("avr109"),
        OsString::from("-P"),
        OsString::from(port),
        OsString::from("-b"),
        OsString::from("57600"),
        OsString::from("-D"),
        OsString::from("-U"),
        OsString::from(format!("flash:w:{image_file}:i")),
    ]
}

fn configure_child_environment(command: &mut Command) {
    command.env_clear();
    let system_root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .unwrap_or_else(|| "C:\\Windows".into());
    command
        .env("SystemRoot", &system_root)
        .env("WINDIR", &system_root)
        .env(
            "PATH",
            PathBuf::from(&system_root)
                .join("System32")
                .join(";")
                .join(&system_root),
        );
    for name in ["TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

fn drain_bounded(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(limit.min(4096));
    let mut buffer = [0u8; 4096];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let keep = count.min(limit.saturating_sub(output.len()));
        output.extend_from_slice(&buffer[..keep]);
    }
    Ok(output)
}

fn bounded_output(first: &[u8], second: &[u8]) -> String {
    let text = String::from_utf8_lossy(first)
        .chars()
        .chain(String::from_utf8_lossy(second).chars())
        .filter(|character| !character.is_control() || *character == ' ')
        .take(1200)
        .collect::<String>();
    text.trim().to_string()
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|error| {
        format!(
            "Could not open required update file {}: {error}",
            path.display()
        )
    })?;
    let length = file
        .metadata()
        .map_err(|error| format!("Could not inspect required update file: {error}"))?
        .len();
    if length > maximum as u64 {
        return Err("A required update file exceeds its size limit.".into());
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read required update file: {error}"))?;
    if bytes.len() > maximum {
        return Err("A required update file exceeds its size limit.".into());
    }
    Ok(bytes)
}

fn validate_version(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_digit()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    {
        return Err("A firmware version field is malformed.".into());
    }
    Ok(())
}

fn validate_device_id(value: &str) -> Result<(), String> {
    if value == "unprovisioned" {
        return Ok(());
    }
    let Some(hex) = value.strip_prefix("srn-") else {
        return Err("A device identity is malformed.".into());
    };
    if hex.len() != 32 || !is_lower_hex(hex) || hex.bytes().all(|byte| byte == b'0') {
        return Err("A device identity is malformed.".into());
    }
    Ok(())
}

fn validate_sha40(value: &str, name: &str) -> Result<(), String> {
    if value.len() != 40 || !is_lower_hex(value) || value.bytes().all(|byte| byte == b'0') {
        return Err(format!(
            "The {name} must be 40 lowercase hexadecimal digits."
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, name: &str) -> Result<(), String> {
    if value.len() != 64 || !is_lower_hex(value) {
        return Err(format!(
            "The {name} must be 64 lowercase hexadecimal digits."
        ));
    }
    Ok(())
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_hex(value: &[u8]) -> Result<Vec<u8>, ()> {
    if !value.len().is_multiple_of(2) {
        return Err(());
    }
    value
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0]).ok_or(())?;
            let low = hex_nibble(pair[1]).ok_or(())?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'A'..=b'F' => Some(value - b'A' + 10),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn next_request_id() -> u32 {
    let next = NEXT_ID.fetch_add(1, Ordering::Relaxed) as u32;
    if next == 0 { 1 } else { next }
}

fn new_id(prefix: &str) -> String {
    let counter = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{time:x}-{counter:x}")
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const HEX: &str = ":0400000001020304F2\n:00000001FF\n";
    const SOURCE_SHA: &str = "1234567890abcdef1234567890abcdef12345678";

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let directory = std::env::temp_dir().join(new_id("updater-test"));
            fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Clone)]
    struct FakeState {
        ports: Vec<PortRecord>,
        bootloader_port: PortRecord,
        application_after_flash: PortRecord,
        observations: Vec<DeviceObservation>,
        inspected_targets: Vec<PortKey>,
        reset_targets: Vec<PortKey>,
        flash_targets: Vec<PortKey>,
        inspection_count: usize,
        flash_calls: usize,
        flash_start_marker: PathBuf,
        flash_marker_had_write_started: bool,
        reset_result: Result<(), FlashError>,
        flash_result: Result<(), FlashError>,
    }
    struct FakeBackend(Arc<Mutex<FakeState>>);
    impl Backend for FakeBackend {
        fn ports(&mut self) -> Result<Vec<PortRecord>, String> {
            Ok(self.0.lock().unwrap().ports.clone())
        }
        fn inspect(&mut self, target: &PortKey) -> Result<DeviceObservation, String> {
            let mut state = self.0.lock().unwrap();
            if !state.ports.iter().any(|port| port.key == *target) {
                return Err(
                    "The scripted target is not present in the current USB topology.".into(),
                );
            }
            state.inspected_targets.push(target.clone());
            let index = state
                .inspection_count
                .min(state.observations.len().saturating_sub(1));
            state.inspection_count += 1;
            Ok(state.observations[index].clone())
        }
        fn reset_to_bootloader(&mut self, target: &PortKey) -> Result<(), FlashError> {
            let mut state = self.0.lock().unwrap();
            if !state.ports.iter().any(|port| port.key == *target) {
                return Err(FlashError {
                    writes_may_have_started: false,
                    message:
                        "The selected application port is not present in the scripted topology."
                            .into(),
                });
            }
            state.reset_targets.push(target.clone());
            state.reset_result.clone()?;
            state.ports = vec![state.bootloader_port.clone()];
            Ok(())
        }
        fn flash(
            &mut self,
            target: &PortKey,
            _image_path: &Path,
            _runtime: &ProgrammerRuntime,
        ) -> Result<(), FlashError> {
            let mut state = self.0.lock().unwrap();
            if !state.ports.iter().any(|port| port.key == *target) {
                return Err(FlashError {
                    writes_may_have_started: false,
                    message:
                        "The selected bootloader port is not present in the scripted topology."
                            .into(),
                });
            }
            state.flash_marker_had_write_started = fs::read(&state.flash_start_marker)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|journal| journal.get("write_may_have_started")?.as_bool())
                .unwrap_or(false);
            if !state.flash_marker_had_write_started {
                return Err(FlashError {
                    writes_may_have_started: false,
                    message:
                        "the durable programmer-start marker was not visible to the fake backend"
                            .into(),
                });
            }
            state.flash_calls += 1;
            state.flash_targets.push(target.clone());
            let result = state.flash_result.clone();
            state.ports = vec![state.application_after_flash.clone()];
            result
        }
    }

    fn target() -> PortKey {
        PortKey {
            name: "COM7".into(),
            kind: "usb".into(),
            serial_number: Some("USB-SERIAL-1".into()),
            product: Some("Pro Micro".into()),
            manufacturer: Some("SparkFun".into()),
            vid: Some(0x1b4f),
            pid: Some(0x9206),
        }
    }

    fn usb_location_path() -> String {
        "USBROOT(0)#USB(3)".into()
    }

    fn application_port() -> PortRecord {
        PortRecord {
            key: target(),
            location_path: Some(usb_location_path()),
        }
    }

    fn bootloader_target() -> PortKey {
        PortKey {
            name: "COM9".into(),
            kind: "usb".into(),
            serial_number: None,
            product: Some("SparkFun AVR109 bootloader".into()),
            manufacturer: Some("SparkFun".into()),
            vid: Some(0x1b4f),
            pid: Some(0x9205),
        }
    }

    fn bootloader_port() -> PortRecord {
        PortRecord {
            key: bootloader_target(),
            location_path: Some(usb_location_path()),
        }
    }

    fn application_after_flash() -> PortRecord {
        let mut key = target();
        key.name = "COM10".into();
        PortRecord {
            key,
            location_path: Some(usb_location_path()),
        }
    }

    fn selected_binding() -> TargetBinding {
        TargetBinding {
            location_path: Some(usb_location_path()),
            serial_number: Some("USB-SERIAL-1".into()),
        }
    }

    fn info(firmware_version: &str, source_sha: &str, generation: u32) -> DeviceObservation {
        DeviceObservation::Protocol3 {
            info: DeviceInfo3 {
                firmware_version: firmware_version.into(),
                source_sha: source_sha.into(),
                board: BOARD.into(),
                mcu: MCU.into(),
                clock_hz: CLOCK_HZ,
                device_id: "srn-0123456789abcdef0123456789abcdef".into(),
                generation,
                assembly: if generation == 0 { 0 } else { 4 },
                known: if generation == 0 { 0 } else { 0x7f },
                negative: 0,
                tare_epoch: 0,
                tare_valid: false,
            },
        }
    }

    fn manifest() -> FirmwarePackageManifest {
        FirmwarePackageManifest {
            schema: 1,
            release_version: APP_RELEASE_VERSION.into(),
            application: APPLICATION.into(),
            board: BOARD.into(),
            mcu: MCU.into(),
            clock_hz: CLOCK_HZ,
            protocol_schema: PROTOCOL_SCHEMA,
            settings_schema: SETTINGS_SCHEMA,
            firmware_version: APP_RELEASE_VERSION.into(),
            source_sha: SOURCE_SHA.into(),
            image_file: "firmware.hex".into(),
            image_sha256: sha256(HEX.as_bytes()),
            image_size: HEX.len() as u64,
            flash_start: APP_FLASH_START,
            flash_end_exclusive: APP_FLASH_END_EXCLUSIVE,
        }
    }

    fn package_input() -> SelectedPackageInput {
        SelectedPackageInput {
            manifest_json: serde_json::to_string(&manifest()).unwrap(),
            firmware_hex: HEX.into(),
        }
    }

    fn service(
        scratch: &Scratch,
        observations: Vec<DeviceObservation>,
    ) -> (UpdaterService, Arc<Mutex<FakeState>>) {
        let runtime_dir = scratch.0.join("resources/distribution/firmware-tools");
        fs::create_dir_all(&runtime_dir).unwrap();
        let executable = b"test-only avrdude executable bytes";
        let config = b"test-only avrdude configuration bytes";
        fs::write(runtime_dir.join("avrdude.exe"), executable).unwrap();
        fs::write(runtime_dir.join("avrdude.conf"), config).unwrap();
        let state = Arc::new(Mutex::new(FakeState {
            ports: vec![application_port()],
            bootloader_port: bootloader_port(),
            application_after_flash: application_after_flash(),
            observations,
            inspected_targets: Vec::new(),
            reset_targets: Vec::new(),
            flash_targets: Vec::new(),
            inspection_count: 0,
            flash_calls: 0,
            flash_start_marker: scratch
                .0
                .join("app-data")
                .join(MANAGER_UPDATE_DIR)
                .join(PENDING_JOURNAL),
            flash_marker_had_write_started: false,
            reset_result: Ok(()),
            flash_result: Ok(()),
        }));
        let updater = UpdaterService::with_backend_and_runtime_pins(
            scratch.0.join("resources"),
            scratch.0.join("app-data"),
            Box::new(FakeBackend(state.clone())),
            RuntimePins {
                executable_sha256: Some(sha256(executable)),
                config_sha256: Some(sha256(config)),
            },
        );
        (updater, state)
    }

    #[test]
    fn bundled_bytes_keep_the_validated_resource_path_without_local_staging() {
        let scratch = Scratch::new();
        let (updater, state) = service(&scratch, Vec::new());
        let package = package_input();
        let prepared = updater
            .prepare_package_bytes(
                package.manifest_json.as_bytes(),
                package.firmware_hex.as_bytes(),
                "bundled_provenance_matched",
                false,
            )
            .unwrap();
        assert_eq!(
            prepared.firmware_hex_path,
            scratch
                .0
                .join("resources/distribution/firmware/firmware.hex")
        );
        assert_eq!(prepared.source_trust, "bundled_provenance_matched");
        assert!(!scratch.0.join("app-data").exists());
        assert_eq!(state.lock().unwrap().flash_calls, 0);
        let bad = package.firmware_hex.replace("F2", "F3");
        assert!(
            updater
                .prepare_package_bytes(
                    package.manifest_json.as_bytes(),
                    bad.as_bytes(),
                    "bundled_provenance_matched",
                    false
                )
                .is_err()
        );
    }

    #[test]
    fn intel_hex_requires_eof_and_valid_checksum() {
        assert_eq!(parse_intel_hex(HEX).unwrap().max_address_exclusive, 4);
        assert!(parse_intel_hex(":0400000001020304F3\n:00000001FF\n").is_err());
        assert!(
            parse_intel_hex(":0400000001020304F2\n")
                .unwrap_err()
                .contains("EOF")
        );
    }

    #[test]
    fn intel_hex_rejects_overlap_eeprom_bootloader_and_post_eof_bytes() {
        assert!(
            parse_intel_hex(":0100000001FE\n:0100000001FE\n:00000001FF\n")
                .unwrap_err()
                .contains("overlaps")
        );
        assert!(
            parse_intel_hex(":02000004008179\n:0100000001FE\n:00000001FF\n")
                .unwrap_err()
                .contains("outside application")
        );
        assert!(
            parse_intel_hex(":01700000018E\n:00000001FF\n")
                .unwrap_err()
                .contains("outside application")
        );
        assert!(
            parse_intel_hex(":00000001FF\n:0100000001FE\n")
                .unwrap_err()
                .contains("after EOF")
        );
    }

    #[test]
    fn intel_hex_rejects_unsupported_record_types_and_bad_extended_addresses() {
        assert!(
            parse_intel_hex(":00000006FA\n:00000001FF\n")
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(
            parse_intel_hex(":020000040001F9\n:00000001FF\n")
                .unwrap_err()
                .contains("outside application")
        );
    }

    #[test]
    fn legacy_or_unidentified_initial_install_requires_manual_board_confirmation() {
        let scratch = Scratch::new();
        let (mut updater, state) = service(&scratch, vec![DeviceObservation::Unidentified]);
        let result = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        assert_eq!(result.status, "requires_manual_board_confirmation");
        assert!(!result.apply_enabled);
        assert_eq!(state.lock().unwrap().flash_calls, 0);
        assert!(
            !scratch
                .0
                .join("app-data")
                .join(MANAGER_UPDATE_DIR)
                .join("backups")
                .exists()
        );
    }

    #[test]
    fn explicit_prepare_replaces_only_unstarted_plans_and_rejects_old_plan_id() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let (mut updater, state) = service(&scratch, vec![initial.clone(), initial]);
        let a = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        let old = a.plan_id.unwrap();
        let b = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        assert_ne!(Some(old.clone()), b.plan_id);
        assert!(
            updater
                .apply(ApplyRequest {
                    plan_id: old,
                    operator_confirmed: true,
                    bootloader_target: None,
                    manual_bootloader_confirmation: false
                })
                .is_err()
        );
        assert_eq!(state.lock().unwrap().flash_calls, 0);
        let marker = updater.pending_journal_path();
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(&marker, b"incomplete reset/write").unwrap();
        assert!(
            updater
                .prepare(PrepareRequest {
                    target: target(),
                    package: Some(package_input()),
                    manual_board_confirmation: false
                })
                .unwrap_err()
                .contains("recovery")
        );
    }

    #[test]
    fn local_package_is_hash_checked_and_operator_confirmation_gates_flash() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let final_state = info(APP_RELEASE_VERSION, SOURCE_SHA, 7);
        let (mut updater, state) = service(&scratch, vec![initial.clone(), initial, final_state]);
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        assert_eq!(prepared.status, "ready");
        assert_eq!(prepared.source_trust, "user_selected_unverified");
        let plan_id = prepared.plan_id.unwrap();
        assert!(
            updater
                .apply(ApplyRequest {
                    plan_id: plan_id.clone(),
                    operator_confirmed: false,
                    bootloader_target: None,
                    manual_bootloader_confirmation: false,
                })
                .is_err()
        );
        assert_eq!(state.lock().unwrap().flash_calls, 0);
        assert!(state.lock().unwrap().reset_targets.is_empty());
        let applied = updater
            .apply(ApplyRequest {
                plan_id,
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert_eq!(applied.status, "verified");
        assert!(!applied.needs_recovery);
        assert_eq!(applied.settings_preserved, Some(true));
        let state = state.lock().unwrap();
        assert_eq!(state.reset_targets, vec![target()]);
        assert_eq!(state.flash_targets, vec![bootloader_target()]);
        assert_eq!(state.inspected_targets.last().unwrap().name, "COM10");
        assert_eq!(state.flash_calls, 1);
    }

    #[test]
    fn unbound_single_bootloader_requires_a_second_explicit_selection() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let final_state = info(APP_RELEASE_VERSION, SOURCE_SHA, 7);
        let (mut updater, state) = service(&scratch, vec![initial.clone(), initial, final_state]);
        state.lock().unwrap().bootloader_port.location_path = None;
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        let plan_id = prepared.plan_id.unwrap();

        let selection = updater
            .apply(ApplyRequest {
                plan_id: plan_id.clone(),
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert_eq!(selection.status, "awaiting_bootloader_confirmation");
        assert!(selection.manual_bootloader_confirmation_required);
        assert_eq!(selection.bootloader_candidates, vec![bootloader_target()]);
        {
            let state = state.lock().unwrap();
            assert_eq!(state.reset_targets, vec![target()]);
            assert!(state.flash_targets.is_empty());
            assert_eq!(
                state.ports,
                vec![PortRecord {
                    key: bootloader_target(),
                    location_path: None
                }]
            );
        }

        let bootloader = selection.bootloader_candidates[0].clone();
        let applied = updater
            .apply(ApplyRequest {
                plan_id,
                operator_confirmed: true,
                bootloader_target: Some(bootloader.clone()),
                manual_bootloader_confirmation: true,
            })
            .unwrap();
        assert_eq!(applied.status, "verified");
        assert!(!applied.needs_recovery);
        let state = state.lock().unwrap();
        assert_eq!(state.reset_targets, vec![target()]);
        assert_eq!(state.flash_targets, vec![bootloader]);
        assert_eq!(state.inspected_targets.last().unwrap().name, "COM10");
    }

    #[test]
    fn possible_write_failure_persists_recovery_and_readback_clears_it() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let final_state = info(APP_RELEASE_VERSION, SOURCE_SHA, 7);
        let (mut updater, state) = service(
            &scratch,
            vec![initial.clone(), initial, final_state.clone(), final_state],
        );
        state.lock().unwrap().flash_result = Err(FlashError {
            writes_may_have_started: true,
            message: "simulated bounded programmer failure".into(),
        });
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        let applied = updater
            .apply(ApplyRequest {
                plan_id: prepared.plan_id.unwrap(),
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert!(applied.needs_recovery);
        assert!(updater.pending_journal_path().exists());
        assert!(state.lock().unwrap().flash_marker_had_write_started);
        let recovered = updater.recover().unwrap();
        assert_eq!(recovered.status, "verified");
        assert!(!recovered.needs_recovery);
        assert!(!updater.pending_journal_path().exists());
    }

    #[test]
    fn pre_programmer_reset_failure_recovers_exact_original_state_as_aborted() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let (mut updater, state) =
            service(&scratch, vec![initial.clone(), initial.clone(), initial]);
        state.lock().unwrap().reset_result = Err(FlashError {
            writes_may_have_started: false,
            message: "simulated port-open failure before reset".into(),
        });
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        let applied = updater
            .apply(ApplyRequest {
                plan_id: prepared.plan_id.unwrap(),
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert_eq!(applied.status, "needs_recovery");
        assert!(applied.needs_recovery);
        assert!(updater.pending_journal_path().exists());
        assert_eq!(state.lock().unwrap().flash_calls, 0);

        let recovered = updater.recover().unwrap();
        assert_eq!(recovered.status, "aborted_no_change");
        assert!(!recovered.needs_recovery);
        assert_eq!(recovered.settings_preserved, Some(true));
        assert!(!updater.pending_journal_path().exists());
    }

    #[test]
    fn initial_install_without_original_protocol3_state_cannot_clear_recovery() {
        let scratch = Scratch::new();
        let (mut updater, state) = service(
            &scratch,
            vec![
                DeviceObservation::Unidentified,
                DeviceObservation::Unidentified,
            ],
        );
        state.lock().unwrap().reset_result = Err(FlashError {
            writes_may_have_started: false,
            message: "simulated port-open failure before reset".into(),
        });
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: true,
            })
            .unwrap();
        let applied = updater
            .apply(ApplyRequest {
                plan_id: prepared.plan_id.unwrap(),
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert!(applied.needs_recovery);
        let recovered = updater.recover().unwrap();
        assert_eq!(recovered.status, "needs_recovery");
        assert!(recovered.needs_recovery);
        assert!(
            recovered
                .message
                .contains("unchanged-state recovery cannot be claimed")
        );
        assert!(updater.pending_journal_path().exists());
        assert_eq!(state.lock().unwrap().flash_calls, 0);
    }

    #[test]
    fn post_programmer_launch_unchanged_original_firmware_remains_in_recovery() {
        let scratch = Scratch::new();
        let initial = info("0.1.0", SOURCE_SHA, 7);
        let (mut updater, state) =
            service(&scratch, vec![initial.clone(), initial.clone(), initial]);
        state.lock().unwrap().flash_result = Err(FlashError {
            writes_may_have_started: true,
            message: "simulated programmer failure after launch".into(),
        });
        let prepared = updater
            .prepare(PrepareRequest {
                target: target(),
                package: Some(package_input()),
                manual_board_confirmation: false,
            })
            .unwrap();
        let applied = updater
            .apply(ApplyRequest {
                plan_id: prepared.plan_id.unwrap(),
                operator_confirmed: true,
                bootloader_target: None,
                manual_bootloader_confirmation: false,
            })
            .unwrap();
        assert!(applied.needs_recovery);
        assert!(state.lock().unwrap().flash_marker_had_write_started);

        let recovered = updater.recover().unwrap();
        assert_eq!(recovered.status, "needs_recovery");
        assert!(recovered.needs_recovery);
        assert!(
            recovered
                .message
                .contains("does not match the selected package")
        );
        assert!(updater.pending_journal_path().exists());
    }

    #[test]
    fn known_topology_conflicts_cannot_be_overridden_by_manual_selection() {
        let binding = selected_binding();
        let mut candidate = bootloader_port();
        candidate.location_path = Some("USBROOT(0)#USB(9)".into());
        candidate.key.serial_number = binding.serial_number.clone();
        assert!(
            resolve_bootloader_target(
                &[candidate.clone()],
                &target(),
                &binding,
                Some(&candidate.key),
                true,
                true,
            )
            .unwrap_err()
            .contains("Manual confirmation cannot override conflicting topology")
        );

        candidate.location_path = binding.location_path.clone();
        candidate.key.serial_number = Some("USB-SERIAL-OTHER".into());
        assert!(
            resolve_bootloader_target(
                &[candidate.clone()],
                &target(),
                &binding,
                Some(&candidate.key),
                true,
                true,
            )
            .unwrap_err()
            .contains("Manual confirmation cannot override conflicting topology")
        );
    }

    #[test]
    fn unique_manual_bootloader_selection_requires_a_missing_descriptor() {
        let binding = selected_binding();
        let mut candidate = bootloader_port();
        candidate.location_path = None;
        candidate.key.serial_number = None;
        assert!(matches!(
            resolve_bootloader_target(
                &[candidate.clone()],
                &target(),
                &binding,
                None,
                false,
                false,
            )
            .unwrap(),
            BootloaderSelection::NeedsManualConfirmation(_)
        ));

        candidate.location_path = Some("USBROOT(0)#USB(9)".into());
        candidate.key.serial_number = Some("USB-SERIAL-OTHER".into());
        assert!(
            resolve_bootloader_target(
                &[candidate.clone()],
                &target(),
                &binding,
                Some(&candidate.key),
                true,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn programmer_arguments_are_fixed_flash_only_and_keep_default_verification() {
        let args = programmer_args(
            "COM7",
            "firmware.hex",
            Path::new("C:\\bundle\\avrdude.conf"),
        );
        let text: Vec<String> = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(text.contains(&"-N".into()));
        assert!(text.contains(&"-D".into()));
        assert!(text.windows(2).any(|pair| pair == ["-p", MCU]));
        assert!(text.windows(2).any(|pair| pair == ["-c", "avr109"]));
        assert!(text.windows(2).any(|pair| pair == ["-P", "COM7"]));
        assert!(text.iter().any(|value| value == "flash:w:firmware.hex:i"));
        assert!(
            !text
                .iter()
                .any(|value| matches!(value.as_str(), "-e" | "-V" | "-F"))
        );
        assert!(!text.iter().any(|value| value.starts_with("eeprom:")
            || value.starts_with("lfuse:")
            || value.starts_with("hfuse:")
            || value.starts_with("efuse:")));
    }

    #[test]
    fn duplicated_usb_serial_descriptor_blocks_preflight() {
        let mut duplicate = target();
        duplicate.name = "COM8".into();
        assert!(
            validate_topology(
                &[
                    application_port(),
                    PortRecord {
                        key: duplicate,
                        location_path: None
                    },
                ],
                &target()
            )
            .unwrap_err()
            .contains("duplicated")
        );
    }
}
