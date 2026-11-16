use std::process::Command;

#[test]
fn support_log_does_not_capture_private_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("session.jsonl");
    let output = Command::new(env!("CARGO_BIN_EXE_sensitivity"))
        .env("SENSITIVITY_LOG_FILE", &log)
        .args([
            "--server-url",
            "https://example.invalid/?signature=private-url-value",
            "--override-sn",
            "private-device-identity",
            "completions",
            "bash",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(log).unwrap();
    assert!(text.contains("backend.started"));
    assert!(text.contains("backend.completed"));
    assert!(!text.contains("private-url-value"));
    assert!(!text.contains("private-device-identity"));
    assert!(!text.contains("example.invalid"));
}
