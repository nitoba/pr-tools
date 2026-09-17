//! Testes end-to-end do comando `prt doctor`.

use std::process::Command;

#[test]
fn non_interactive_doctor_should_run_checks_and_fail_when_requirements_are_missing() {
    let config_home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_prt"))
        .arg("doctor")
        .current_dir(config_home.path())
        .env("PATH", "")
        .env("XDG_CONFIG_HOME", config_home.path())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}");
    assert!(stdout.contains("[FALHA]"), "stdout:\n{stdout}");
    assert!(!stdout.contains("em implementação"), "stdout:\n{stdout}");
}
