use selfrionette_core::{input_calibration::InputQuality, setup::*, setup_persistence::*};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files {
    root: PathBuf,
    context: PathBuf,
    binding: PathBuf,
}
impl Files {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "setup-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(root.is_absolute());
        fs::create_dir(&root).unwrap();
        Self {
            context: root.join("context.srnc"),
            binding: root.join("profile.srnb"),
            root,
        }
    }
    fn run(&self, binding: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"));
        command.arg("setup-inspect").arg(&self.context);
        if binding {
            command.arg(&self.binding);
        }
        command.output().unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        for path in [&self.context, &self.binding] {
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        fs::remove_dir(&self.root).unwrap();
    }
}
fn context() -> CurrentContext {
    let evidence = Evidence {
        origin: Origin::Synthetic,
        quality: InputQuality::Usable,
        reference: "synthetic-test-only".into(),
    };
    CurrentContext {
        setup: DeviceSetup {
            device_id: "srn-0123456789abcdef0123456789abcdef".parse().unwrap(),
            assembly_revision: 1,
            config_revision: 1,
            settings_generation: 1,
            channels: std::array::from_fn(|wire| ChannelOrientation {
                logical_axis: Some(wire),
                location: Some(format!("fixture-{wire}")),
                positive_direction: Some(format!("direction-{wire}")),
                polarity: Some(if wire == 1 { -1 } else { 1 }),
            }),
            mapping_validated: true,
            evidence,
        },
        firmware: FirmwareBinding {
            version: "synthetic-v2".into(),
            source_revision: "1".repeat(40),
            sample_domain: SampleDomain::LegacyFirmwareV2,
        },
        operator: OperatorContext {
            operator: "test-person".into(),
            posture: "test-posture".into(),
            grip: "test-grip".into(),
        },
    }
}
fn binding(c: &CurrentContext) -> ProfileBinding {
    ProfileBinding {
        profile_revision: 1,
        parameter_record: "parameters-not-loaded-by-this-command".into(),
        setup: c.setup.clone(),
        firmware: c.firmware.clone(),
        operator: c.operator.clone(),
        evidence: c.setup.evidence.clone(),
    }
}

#[test]
fn callable_file_boundary_compares_binding_without_claiming_usable_input() {
    let files = Files::new();
    let c = context();
    let p = binding(&c);
    fs::write(&files.context, encode_context(&c).unwrap()).unwrap();
    save_binding_new(&files.binding, &p).unwrap();
    let before = fs::read(&files.binding).unwrap();
    assert!(save_binding_new(&files.binding, &p).is_err());
    assert_eq!(fs::read(&files.binding).unwrap(), before);
    assert_eq!(load_binding(&files.binding).unwrap(), p);
    let output = files.run(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for field in [
        "profile_binding_compatible",
        "synthetic_only",
        "fresh_personal_neutral_required",
        "personal_calibration_required",
        "device_setup_required",
    ] {
        assert_eq!(report[field], true, "{field}");
    }
    for field in ["hardware_verified", "control_ready", "parameters_loaded"] {
        assert_eq!(report[field], false, "{field}");
    }
    assert_eq!(fs::read(&files.binding).unwrap(), before);
    let mut changed = c;
    changed.setup.settings_generation += 1;
    fs::write(&files.context, encode_context(&changed).unwrap()).unwrap();
    let output = files.run(true);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("RevalidationRequired"));
}

#[test]
fn partial_unknown_configuration_remains_inspectable_with_reason() {
    let files = Files::new();
    let mut c = context();
    c.setup.mapping_validated = false;
    c.setup.channels[0].polarity = None;
    c.setup.evidence.quality = InputQuality::Unknown;
    fs::write(&files.context, encode_context(&c).unwrap()).unwrap();
    let output = files.run(false);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["mapping_complete"], false);
    assert_eq!(report["setup_failure"], "IncompleteSetup");
    assert!(report["profile_binding_compatible"].is_null());
    assert_eq!(
        load_context(&files.context).unwrap().setup.channels[0].polarity,
        None
    );
}

#[test]
fn reload_rejects_invalid_files_and_never_falls_back_to_previous_data() {
    let files = Files::new();
    let c = context();
    fs::write(&files.context, encode_context(&c).unwrap()).unwrap();
    save_binding_new(&files.binding, &binding(&c)).unwrap();
    let original = fs::read(&files.binding).unwrap();
    for bytes in [
        b"srn-profile-binding,1\n".to_vec(),
        vec![b'x'; 32769],
        vec![0xff],
        original[..original.len() / 2].to_vec(),
    ] {
        fs::write(&files.binding, bytes).unwrap();
        assert!(load_binding(&files.binding).is_err());
        assert!(!files.run(true).status.success());
    }
    fs::write(&files.binding, original).unwrap();
    fs::write(
        &files.context,
        b"srn-preview,1\nhardware_verified,0\ncontrol_ready,0\n",
    )
    .unwrap();
    assert!(!files.run(true).status.success());
}

#[test]
fn inspect_rejects_serial_arguments_and_extra_paths() {
    for args in [vec!["--port", "COM1"], vec!["a", "b", "c"], vec![]] {
        let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
            .arg("setup-inspect")
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}
