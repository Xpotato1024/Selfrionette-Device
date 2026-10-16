//! File-only inspection. No discovery, serial, provisioning or live apply.
use crate::CliError;
use selfrionette_core::{setup::*, setup_persistence::*};
use serde_json::json;
use std::{io::Write, path::Path};

pub(crate) fn run(args: &[String]) -> Result<(), CliError> {
    if !(1..=2).contains(&args.len()) || args.iter().any(|p| p.starts_with('-')) {
        return Err(CliError::Usage(
            "setup-inspect <context.srnc> [profile-binding.srnb] (files only)".into(),
        ));
    }
    let current = load_context(Path::new(&args[0])).map_err(failure)?;
    let binding = args
        .get(1)
        .map(|path| load_binding(Path::new(path)))
        .transpose()
        .map_err(failure)?;
    if let Some(profile) = &binding {
        profile.check_compatibility(&current).map_err(failure)?;
    }
    let complete = current.setup.require_validated_mapping().is_ok();
    let report = json!({
        "schema_version": 1,
        "device_id": current.setup.device_id.to_string(),
        "assembly_revision": current.setup.assembly_revision,
        "config_revision": current.setup.config_revision,
        "settings_generation": current.setup.settings_generation,
        "sample_domain": domain_name(current.firmware.sample_domain),
        "firmware": current.firmware.version,
        "source_revision": current.firmware.source_revision,
        "operator": current.operator.operator, "posture": current.operator.posture, "grip": current.operator.grip,
        "mapping_complete": current.setup.is_complete(), "mapping_asserted_validated": current.setup.mapping_validated,
        "setup_metadata_complete": complete,
        "setup_failure": current.setup.require_validated_mapping().err().map(|e| e.to_string()),
        "device_setup_required": true,
        "profile_binding_compatible": binding.as_ref().map(|_| true),
        "profile_revision": binding.as_ref().map(|p| p.profile_revision),
        "parameter_record": binding.as_ref().map(|p| &p.parameter_record),
        "synthetic_only": current.setup.evidence.origin == Origin::Synthetic,
        "parameters_loaded": false, "fresh_personal_neutral_required": true,
        "personal_calibration_required": true,
        "hardware_verified": false, "control_ready": false,
        "scope": "structural metadata inspection only; no parameter load, acquisition or physical certification"
    });
    let mut output = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &report).map_err(failure)?;
    output.write_all(b"\n").map_err(failure)?;
    Ok(())
}
fn failure(e: impl std::fmt::Display) -> CliError {
    CliError::Failure(format!("setup binding: {e}"))
}
