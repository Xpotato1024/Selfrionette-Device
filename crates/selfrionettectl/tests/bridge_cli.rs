use std::{
    io::Write,
    process::{Command, Stdio},
};

fn bridge(input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
        .arg("bridge")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // An invalid overlong line may close its pipe while the parent is writing.
    let _ = stdin.write_all(input);
    drop(stdin);
    child.wait_with_output().unwrap()
}

#[test]
fn stdio_status_handshake_echoes_request_and_uses_monotonic_emission_clock() {
    let output = bridge(concat!(
        "{\"schema\":\"selfrionette-bridge-command/v1\",\"request_id\":1,\"action\":\"status\"}\n",
        "{\"schema\":\"selfrionette-bridge-command/v1\",\"request_id\":2,\"action\":\"status\"}\n",
    ).as_bytes());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let events: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 3);
    let mut previous = 0;
    for (request, event) in events.iter().enumerate() {
        assert_eq!(event["schema"], "selfrionette-bridge-event/v1");
        assert_eq!(event["event"], "status");
        assert_eq!(event["request_id"], request as u64);
        assert_eq!(event["phase"], "unconfigured");
        assert_eq!(event["host_clock"], "bridge_process_elapsed_us");
        assert_eq!(event["ready"], false);
        assert!(event["source"].is_null());
        assert!(event["bridge_revision"].is_string());
        let emitted = event["host_emitted_us"].as_u64().unwrap();
        assert!(emitted >= previous);
        previous = emitted;
    }
}

#[test]
fn malformed_and_oversized_json_fail_closed_without_a_configured_port() {
    for input in [
        b"{\"schema\":\"selfrionette-bridge-command/v1\",\"request_id\":1,\"action\":\"start\"}\n".to_vec(),
        b"{\"schema\":\"selfrionette-bridge-command/v1\",\"request_id\":1,\"action\":\"status\",\"confirmed\":true}\n".to_vec(),
        vec![b'x';64*1024+1],
    ] {
        let output=bridge(&input);
        assert!(!output.status.success());
        let events:Vec<serde_json::Value>=String::from_utf8(output.stdout).unwrap().lines().map(|line|serde_json::from_str(line).unwrap()).collect();
        let error=events.iter().find(|event|event["event"]=="error").expect("typed failure event");
        assert_eq!(error["fatal"],true);
        assert_eq!(error["ready"],false);
        assert!(events.iter().all(|event|event["event"]!="sample"));
    }
    let result = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
        .args([
            "bridge",
            "--revision",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();
    assert_eq!(
        result.status.code(),
        Some(2),
        "revision must come from build provenance"
    );
}
