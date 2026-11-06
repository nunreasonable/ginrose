#![cfg(unix)]

use std::{fs, process::Command};

#[test]
fn rosemary_install_reports_config_save_failure() {
    let root = std::env::temp_dir().join(format!("ginrose-rosemary-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let source = root.join("rosemary-source");
    fs::write(&source, b"runtime").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ginrose"))
        .arg("rosemary")
        .arg(&source)
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", "/dev/null")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::remove_dir_all(&root).unwrap();

    assert!(
        !output.status.success(),
        "install reported success despite the config save failure\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("Rosemary installed at"),
        "success message was printed despite the config save failure\nstderr: {stderr}"
    );
}
