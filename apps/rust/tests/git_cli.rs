//! Testes end-to-end do contexto Git usado pelo comando `prt desc`.

use std::path::Path;
use std::process::{Command, Stdio};

fn git(directory: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(directory)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {} falhou", args.join(" "));
}

#[test]
fn nonexistent_source_branch_should_fail_instead_of_using_empty_context() {
    let repository = tempfile::tempdir().unwrap();
    git(repository.path(), &["init"]);
    git(
        repository.path(),
        &["config", "user.email", "audit@example.com"],
    );
    git(repository.path(), &["config", "user.name", "Audit"]);
    std::fs::write(repository.path().join("tracked.txt"), "baseline\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "baseline"]);
    git(repository.path(), &["branch", "-M", "main"]);

    let output = Command::new(env!("CARGO_BIN_EXE_prt"))
        .args([
            "desc",
            "--source",
            "__audit_nonexistent_branch__",
            "--dry-run",
            "--no-copy",
        ])
        .current_dir(repository.path())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("git"), "stderr:\n{stderr}");
}
