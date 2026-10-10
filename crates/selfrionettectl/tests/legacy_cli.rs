use std::process::Command;

#[test]
fn real_cli_help_and_missing_port_never_enter_live_operation() {
    for command in ["legacy-monitor", "legacy-measure"] {
        let help = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(
            String::from_utf8(help.stdout)
                .unwrap()
                .contains("No implicit COM5")
        );
        let missing = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
            .arg(command)
            .output()
            .unwrap();
        assert_eq!(missing.status.code(), Some(2));
        assert!(
            String::from_utf8(missing.stderr)
                .unwrap()
                .contains("--port must be explicit")
        );
    }
}

#[test]
fn powershell_help_false_retains_required_port_gate() {
    let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
        .args(["legacy-monitor", "--powershell-args", "-Help:$false"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("--port must be explicit")
    );
}

#[test]
fn malformed_options_are_rejected_before_open_even_with_explicit_port() {
    let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
        .args([
            "legacy-measure",
            "--port",
            "this-is-not-opened",
            "--sensor",
            "8",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("sensor must be 1..7"));
    assert!(!error.contains("failed to open"));
}
