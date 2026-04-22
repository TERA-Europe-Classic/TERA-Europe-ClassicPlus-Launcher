use std::process::Command;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_gpk-patch-converter")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin_path())
        .args(args)
        .output()
        .expect("run gpk-patch-converter")
}

#[test]
fn cli_help_exits_zero_and_prints_usage() {
    let output = run(&["--help"]);

    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--reference-vanilla"));
    assert!(stdout.contains("--output-bundle-dir"));
}

#[test]
fn cli_missing_argument_exits_one() {
    let output = run(&["--reference-vanilla", "vanilla.gpk"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Missing required flag: --modded-gpk"));
}

#[test]
fn cli_validated_inputs_exit_two_and_report_unimplemented() {
    let temp = tempfile::tempdir().expect("tempdir");
    let vanilla = temp.path().join("vanilla.gpk");
    let modded = temp.path().join("modded.gpk");
    std::fs::write(&vanilla, b"vanilla").expect("write vanilla");
    std::fs::write(&modded, b"modded").expect("write modded");

    let bundle_dir = tempfile::tempdir().expect("tempdir");
    let bundle_dir = bundle_dir.path().to_string_lossy().to_string();
    let vanilla = vanilla.to_string_lossy().to_string();
    let modded = modded.to_string_lossy().to_string();

    let output = run(&[
        "--reference-vanilla",
        &vanilla,
        "--modded-gpk",
        &modded,
        "--mod-id",
        "foglio1024.ui-remover-flight-gauge",
        "--output-bundle-dir",
        &bundle_dir,
    ]);

    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("manifest-path:"));
    assert!(stdout.contains("payload-dir:"));
    assert!(stdout.contains(&bundle_dir));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not implemented yet"));
}

#[test]
fn cli_missing_input_file_exits_one() {
    let bundle_dir = tempfile::tempdir().expect("tempdir");
    let bundle_dir = bundle_dir.path().to_string_lossy().to_string();

    let output = run(&[
        "--reference-vanilla",
        "missing-vanilla.gpk",
        "--modded-gpk",
        "missing-modded.gpk",
        "--mod-id",
        "foglio1024.ui-remover-flight-gauge",
        "--output-bundle-dir",
        &bundle_dir,
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--reference-vanilla does not exist"));
}
