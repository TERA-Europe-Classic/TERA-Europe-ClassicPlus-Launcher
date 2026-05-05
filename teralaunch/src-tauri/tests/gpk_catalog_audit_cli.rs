use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_gpk-catalog-audit")
}

fn valid_gpk_header(file_version: u16, license_version: u16) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    bytes[0..4].copy_from_slice(&0x9E2A83C1u32.to_le_bytes());
    bytes[4..6].copy_from_slice(&file_version.to_le_bytes());
    bytes[6..8].copy_from_slice(&license_version.to_le_bytes());
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn serve_once(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("server addr");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write headers");
        stream.write_all(&body).expect("write body");
    });
    format!("http://{addr}/S1UI_Test.gpk")
}

#[test]
fn cli_writes_markdown_report_for_catalog_gpk_rows() {
    let temp = tempfile::tempdir().expect("tempdir");
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");

    std::fs::write(
        &catalog_path,
        r#"{
          "version": 1,
          "updated_at": "2026-04-30T00:00:00Z",
          "mods": [
            {
              "id": "classicplus.shinra",
              "kind": "external",
              "name": "Shinra",
              "author": "TERA Europe Classic",
              "short_description": "Meter",
              "version": "1",
              "download_url": "https://example.com/shinra.zip",
              "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            {
              "id": "pantypon.pink-chat-window",
              "kind": "gpk",
              "name": "Pink Chat Window",
              "author": "pantypon",
              "short_description": "Pink chat window.",
              "version": "1",
              "download_url": "https://example.com/S1UI_Chat2.gpk",
              "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              "target_patch": "v32.04",
              "composite_flag": true
            },
            {
              "id": "foglio1024.ui-remover-flight-gauge",
              "kind": "gpk",
              "name": "UI Remover: Flight Gauge",
              "author": "foglio1024",
              "short_description": "Hides the flight stamina bar.",
              "version": "1",
              "download_url": "https://example.com/S1UI_ProgressBar.gpk",
              "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
              "target_patch": "v100.02",
              "composite_flag": true
            }
          ]
        }"#,
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = std::fs::read_to_string(&report_path).expect("report written");
    assert!(report.contains("GPK Catalog Audit"));
    assert!(report.contains("publish-x64-rebuild-required"));
    assert!(report.contains("structural-manifest-required"));
    assert!(report.contains("foglio1024.ui-remover-flight-gauge"));
    assert!(!report.contains("classicplus.shinra"));
}

#[test]
fn cli_adds_cached_header_facts_when_cache_dir_is_provided() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).expect("cache dir");
    let sha256 = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    std::fs::write(
        cache_dir.join(format!("{sha256}.gpk")),
        valid_gpk_header(897, 0),
    )
    .expect("write cached header");
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");

    std::fs::write(
        &catalog_path,
        format!(
            r#"{{
              "version": 1,
              "updated_at": "2026-04-30T00:00:00Z",
              "mods": [{{
                "id": "tester.cached-header",
                "kind": "gpk",
                "name": "Cached Header",
                "author": "Tester",
                "short_description": "Cached header audit.",
                "version": "1",
                "download_url": "https://example.com/S1UI_Test.gpk",
                "sha256": "{sha256}",
                "target_patch": "v100.02",
                "composite_flag": true
              }}]
            }}"#
        ),
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
            "--cache-dir",
            cache_dir.to_str().expect("cache utf8"),
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report = std::fs::read_to_string(&report_path).expect("report written");
    assert!(report.contains("Header"));
    assert!(report.contains("FileVersion 897"));
    assert!(report.contains("LicenseVersion 0"));
    assert!(report.contains("header-arch x64"));
}

#[test]
fn cli_downloads_missing_cached_gpk_by_sha_before_header_audit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).expect("cache dir");
    let body = valid_gpk_header(897, 7).to_vec();
    let sha256 = sha256_hex(&body);
    let download_url = serve_once(body);
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");

    std::fs::write(
        &catalog_path,
        format!(
            r#"{{
              "version": 1,
              "updated_at": "2026-04-30T00:00:00Z",
              "mods": [{{
                "id": "tester.download-header",
                "kind": "gpk",
                "name": "Downloaded Header",
                "author": "Tester",
                "short_description": "Downloaded header audit.",
                "version": "1",
                "download_url": "{download_url}",
                "sha256": "{sha256}",
                "target_patch": "v100.02",
                "composite_flag": true
              }}]
            }}"#
        ),
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .env("GPK_CATALOG_AUDIT_ALLOW_LOOPBACK", "1")
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
            "--cache-dir",
            cache_dir.to_str().expect("cache utf8"),
            "--download-missing",
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cache_dir.join(format!("{sha256}.gpk")).exists());
    let report = std::fs::read_to_string(&report_path).expect("report written");
    assert!(report.contains("FileVersion 897"));
    assert!(report.contains("LicenseVersion 7"));
    assert!(report.contains("header-arch x64"));
}

#[test]
fn cli_rejects_malformed_sha_before_download_or_cache_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).expect("cache dir");
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");
    let bad_sha = "../not-a-sha";

    std::fs::write(
        &catalog_path,
        format!(
            r#"{{
              "version": 1,
              "updated_at": "2026-04-30T00:00:00Z",
              "mods": [{{
                "id": "tester.bad-sha",
                "kind": "gpk",
                "name": "Bad SHA",
                "author": "Tester",
                "short_description": "Bad SHA audit.",
                "version": "1",
                "download_url": "http://127.0.0.1:9/unreachable.gpk",
                "sha256": "{bad_sha}",
                "target_patch": "v100.02",
                "composite_flag": true
              }}]
            }}"#
        ),
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
            "--cache-dir",
            cache_dir.to_str().expect("cache utf8"),
            "--download-missing",
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid SHA-256 for tester.bad-sha"),
        "{stderr}"
    );
    assert!(!temp.path().join("not-a-sha.gpk").exists());
    assert!(!report_path.exists());
}

#[test]
fn cli_rejects_malformed_sha_before_cache_header_read() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).expect("cache dir");
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");

    std::fs::write(
        &catalog_path,
        r#"{
          "version": 1,
          "updated_at": "2026-04-30T00:00:00Z",
          "mods": [{
            "id": "tester.bad-cache-sha",
            "kind": "gpk",
            "name": "Bad Cache SHA",
            "author": "Tester",
            "short_description": "Bad cache SHA audit.",
            "version": "1",
            "download_url": "https://example.com/S1UI_Test.gpk",
            "sha256": "../not-a-sha",
            "target_patch": "v100.02",
            "composite_flag": true
          }]
        }"#,
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
            "--cache-dir",
            cache_dir.to_str().expect("cache utf8"),
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid SHA-256 for tester.bad-cache-sha"),
        "{stderr}"
    );
    assert!(!report_path.exists());
}

#[test]
fn cli_rejects_non_github_download_url_when_caching_missing_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let cache_dir = temp.path().join("cache");
    std::fs::create_dir(&cache_dir).expect("cache dir");
    let catalog_path = temp.path().join("catalog.json");
    let report_path = temp.path().join("report.md");
    let body = valid_gpk_header(897, 17).to_vec();
    let sha256 = sha256_hex(&body);

    std::fs::write(
        &catalog_path,
        format!(
            r#"{{
              "version": 1,
              "updated_at": "2026-04-30T00:00:00Z",
              "mods": [{{
                "id": "tester.localhost-url",
                "kind": "gpk",
                "name": "Localhost URL",
                "author": "Tester",
                "short_description": "Localhost URL audit.",
                "version": "1",
                "download_url": "http://127.0.0.1:9/S1UI_Test.gpk",
                "sha256": "{sha256}",
                "target_patch": "v100.02",
                "composite_flag": true
              }}]
            }}"#
        ),
    )
    .expect("write catalog");

    let output = Command::new(bin_path())
        .args([
            "--catalog",
            catalog_path.to_str().expect("catalog utf8"),
            "--out",
            report_path.to_str().expect("report utf8"),
            "--cache-dir",
            cache_dir.to_str().expect("cache utf8"),
            "--download-missing",
        ])
        .output()
        .expect("run gpk-catalog-audit");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Untrusted download URL"), "{stderr}");
    assert!(!cache_dir.join(format!("{sha256}.gpk")).exists());
    assert!(!report_path.exists());
}
