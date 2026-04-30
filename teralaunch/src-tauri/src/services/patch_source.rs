use crate::domain::{FileHashAlgorithm, FileInfo, FileInstallMode};
use rusqlite::{Connection, OptionalExtension};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use url::Url;

pub const MAX_V100_DB_CAB_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_V100_DB_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_V100_PATCH_CAB_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_V100_PATCH_OUTPUT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchSourceKind {
    JsonManifest,
    V100Static,
}

impl PatchSourceKind {
    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "v100" | "v100_static" | "static_ini" => Self::V100Static,
            _ => Self::JsonManifest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V100VersionManifest {
    pub version: u32,
    pub dl_root: String,
    pub db_file: String,
}

pub fn parse_v100_version_ini(contents: &str) -> Result<V100VersionManifest, String> {
    let ini = ini::Ini::load_from_str(contents)
        .map_err(|e| format!("Failed to parse v100 version.ini: {}", e))?;
    let download = ini
        .section(Some("Download"))
        .ok_or_else(|| "version.ini missing [Download] section".to_string())?;

    let version = download
        .get("Version")
        .ok_or_else(|| "version.ini missing Download.Version".to_string())?
        .trim()
        .parse::<u32>()
        .map_err(|e| format!("Invalid Download.Version in version.ini: {}", e))?;
    let dl_root = normalize_relative_path(
        download
            .get("DL root")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "version.ini missing Download.DL root".to_string())?,
    )?;
    let db_file = normalize_relative_path(
        download
            .get("DB file")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "version.ini missing Download.DB file".to_string())?,
    )?;

    Ok(V100VersionManifest {
        version,
        dl_root,
        db_file,
    })
}

pub fn plan_v100_updates_from_db(
    manifest: &V100VersionManifest,
    patch_base_url: &str,
    db_path: &Path,
    game_root: &Path,
) -> Result<Vec<FileInfo>, String> {
    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open v100 DB: {}", e))?;
    let mut stmt = conn
        .prepare("SELECT id, path FROM file_info ORDER BY id")
        .map_err(|e| format!("Failed to read v100 file_info table: {}", e))?;
    let file_rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query v100 file_info rows: {}", e))?;

    let mut files = Vec::new();
    for row in file_rows {
        let (id, raw_path) = row.map_err(|e| format!("Invalid v100 file_info row: {}", e))?;
        let Some(version_info) = latest_file_version(&conn, id)? else {
            continue;
        };
        if version_info.size < 0 || version_info.hash.trim().is_empty() {
            continue;
        }

        let relative_path = normalize_relative_path(&raw_path)?;
        let expected_size = version_info.size as u64;
        validate_v100_patch_output_size(id, expected_size)?;
        let local_path = game_root.join(&relative_path);
        if is_local_file_current(&local_path, expected_size, &version_info.hash)? {
            continue;
        }

        let cab_size = cab_download_size(&conn, id, version_info.version)?.unwrap_or(expected_size);
        validate_v100_patch_cab_size(id, cab_size)?;
        let cab_name = format!("{}-{}.cab", id, version_info.version);
        let cab_url = join_url(
            patch_base_url,
            &format!("{}/{}", manifest.dl_root, cab_name),
        );

        files.push(FileInfo {
            path: relative_path,
            hash: version_info.hash.to_ascii_lowercase(),
            size: cab_size,
            url: cab_url,
            existing_size: 0,
            download_path: Some(format!("$Patch/v100/{}/{}", manifest.dl_root, cab_name)),
            output_size: Some(expected_size),
            hash_algorithm: FileHashAlgorithm::Md5,
            install_mode: FileInstallMode::LzmaCab,
        });
    }

    Ok(files)
}

#[cfg(test)]
pub fn decompress_lzma_file_to_path(input_path: &Path, output_path: &Path) -> Result<(), String> {
    decompress_lzma_file_to_path_with_limit(input_path, output_path, None)
}

pub fn decompress_lzma_file_to_path_with_limit(
    input_path: &Path,
    output_path: &Path,
    max_output_size: Option<u64>,
) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let input = File::open(input_path).map_err(|e| format!("Failed to open LZMA CAB: {}", e))?;
    let output = File::create(output_path)
        .map_err(|e| format!("Failed to create decompressed output: {}", e))?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    match max_output_size {
        Some(limit) => {
            let mut limited_writer = LimitedWriter::new(writer, limit);
            lzma_rs::lzma_decompress(&mut reader, &mut limited_writer)
        }
        None => lzma_rs::lzma_decompress(&mut reader, &mut writer),
    }
    .map_err(|e| format!("Failed to decompress v100 LZMA CAB: {}", e))
}

pub fn validate_v100_db_cab_size(byte_len: usize) -> Result<(), String> {
    if byte_len > MAX_V100_DB_CAB_BYTES {
        return Err(format!(
            "v100 server DB CAB is too large: {} bytes exceeds {} byte limit",
            byte_len, MAX_V100_DB_CAB_BYTES
        ));
    }
    Ok(())
}

fn validate_v100_patch_cab_size(id: i64, byte_len: u64) -> Result<(), String> {
    if byte_len > MAX_V100_PATCH_CAB_BYTES {
        return Err(format!(
            "v100 patch CAB for file id {} is too large: {} bytes exceeds {} byte limit",
            id, byte_len, MAX_V100_PATCH_CAB_BYTES
        ));
    }
    Ok(())
}

fn validate_v100_patch_output_size(id: i64, byte_len: u64) -> Result<(), String> {
    if byte_len > MAX_V100_PATCH_OUTPUT_BYTES {
        return Err(format!(
            "v100 patch output for file id {} is too large: {} bytes exceeds {} byte limit",
            id, byte_len, MAX_V100_PATCH_OUTPUT_BYTES
        ));
    }
    Ok(())
}

pub fn v100_patch_base_url(configured: &str, api_base_url: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.trim_end_matches('/').to_string();
    }
    format!("{}/public/patch", api_base_url.trim_end_matches('/'))
}

pub fn join_patch_url(base: &str, relative: &str) -> String {
    join_url(base, relative)
}

pub fn validate_v100_patch_url(url: &str, patch_base_url: &str) -> Result<(), String> {
    let candidate =
        Url::parse(url).map_err(|e| format!("Invalid v100 patch URL '{}': {}", url, e))?;
    let base = Url::parse(patch_base_url)
        .map_err(|e| format!("Invalid v100 patch base URL '{}': {}", patch_base_url, e))?;

    if candidate.scheme() != base.scheme()
        || candidate.host_str() != base.host_str()
        || candidate.port_or_known_default() != base.port_or_known_default()
    {
        return Err(format!(
            "v100 patch URL '{}' is outside configured patch host '{}'",
            url, patch_base_url
        ));
    }

    let base_path = base.path().trim_end_matches('/');
    let candidate_path = candidate.path();
    if candidate_path != base_path && !candidate_path.starts_with(&format!("{}/", base_path)) {
        return Err(format!(
            "v100 patch URL '{}' is outside configured patch path '{}'",
            url, patch_base_url
        ));
    }

    Ok(())
}

#[cfg(test)]
pub fn md5_hex(bytes: &[u8]) -> String {
    format!("{:x}", md5::compute(bytes))
}

pub fn md5_file_hex(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open file for MD5: {}", e))?;
    let mut context = md5::Context::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read file for MD5: {}", e))?;
        if read == 0 {
            break;
        }
        context.consume(&buffer[..read]);
    }
    Ok(format!("{:x}", context.compute()))
}

#[derive(Debug)]
struct VersionInfo {
    version: i64,
    size: i64,
    hash: String,
}

fn latest_file_version(conn: &Connection, id: i64) -> Result<Option<VersionInfo>, String> {
    conn.query_row(
        "SELECT version, size, hash FROM file_version WHERE id = ?1 ORDER BY version DESC LIMIT 1",
        [id],
        |row| {
            Ok(VersionInfo {
                version: row.get(0)?,
                size: row.get(1)?,
                hash: row.get(2)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("Failed to query v100 file_version for id {}: {}", id, e))
}

fn cab_download_size(conn: &Connection, id: i64, new_version: i64) -> Result<Option<u64>, String> {
    let size = conn
        .query_row(
            "SELECT size FROM file_size WHERE id = ?1 AND new_ver = ?2 ORDER BY CASE WHEN org_ver = -1 THEN 0 ELSE 1 END LIMIT 1",
            (id, new_version),
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| format!("Failed to query v100 file_size for id {}: {}", id, e))?;

    size.map(|value| {
        u64::try_from(value).map_err(|_| format!("Invalid negative CAB size for file id {}", id))
    })
    .transpose()
}

fn is_local_file_current(
    path: &Path,
    expected_size: u64,
    expected_md5: &str,
) -> Result<bool, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(false),
    };
    if metadata.len() != expected_size {
        return Ok(false);
    }
    Ok(md5_file_hex(path)?.eq_ignore_ascii_case(expected_md5))
}

fn normalize_relative_path(raw_path: &str) -> Result<String, String> {
    let normalized = raw_path.replace('\\', "/");
    let trimmed = normalized.trim_start_matches('/');
    let path = Path::new(trimmed);
    if trimmed.is_empty() || path.is_absolute() {
        return Err(format!("Invalid v100 file path: {}", raw_path));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("Path traversal in v100 file path: {}", raw_path));
    }
    Ok(trimmed.to_string())
}

fn join_url(base: &str, relative: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        relative.trim_start_matches('/')
    )
}

struct LimitedWriter<W> {
    inner: W,
    limit: u64,
    written: u64,
}

impl<W> LimitedWriter<W> {
    fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            limit,
            written: 0,
        }
    }
}

impl<W: Write> Write for LimitedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let new_total = self.written.saturating_add(buf.len() as u64);
        if new_total > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "decompressed v100 payload exceeds {} byte limit",
                    self.limit
                ),
            ));
        }
        let written = self.inner.write(buf)?;
        self.written = self.written.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::fs;
    use std::io::{Cursor, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn patch_source_kind_defaults_to_json_and_accepts_v100_static() {
        assert_eq!(
            PatchSourceKind::from_config_value(""),
            PatchSourceKind::JsonManifest
        );
        assert_eq!(
            PatchSourceKind::from_config_value("json"),
            PatchSourceKind::JsonManifest
        );
        assert_eq!(
            PatchSourceKind::from_config_value("v100_static"),
            PatchSourceKind::V100Static
        );
    }

    #[test]
    fn parse_v100_version_ini_reads_download_section() {
        let manifest = parse_v100_version_ini(
            r#"
[Download]
Retry=3
Wait=1000
Version=7
DL root=patch
DB file=db/server.db.7.cab

[CheckHash]
count=0
"#,
        )
        .expect("version.ini should parse");

        assert_eq!(manifest.version, 7);
        assert_eq!(manifest.dl_root, "patch");
        assert_eq!(manifest.db_file, "db/server.db.7.cab");
    }

    #[test]
    fn parse_v100_version_ini_rejects_traversal_paths() {
        let err = parse_v100_version_ini(
            r#"
[Download]
Version=7
DL root=../patch
DB file=db/server.db.7.cab
"#,
        )
        .expect_err("manifest path traversal should fail");

        assert!(err.contains("Path traversal in v100 file path"));
    }

    #[test]
    fn v100_patch_base_url_defaults_to_public_patch_under_api_base() {
        assert_eq!(
            v100_patch_base_url("", "http://157.90.107.2:8090"),
            "http://157.90.107.2:8090/public/patch"
        );
        assert_eq!(
            v100_patch_base_url("https://cdn.example.com/patch/", "http://ignored"),
            "https://cdn.example.com/patch"
        );
    }

    #[test]
    fn validate_v100_patch_url_allows_configured_http_base_only() {
        let base = "http://157.90.107.2:8090/public/patch";
        assert!(validate_v100_patch_url(
            "http://157.90.107.2:8090/public/patch/patch/1-1.cab",
            base,
        )
        .is_ok());
        assert!(
            validate_v100_patch_url("http://157.90.107.2:8090/admin/patch/1-1.cab", base,).is_err()
        );
        assert!(
            validate_v100_patch_url("http://evil.example/public/patch/patch/1-1.cab", base,)
                .is_err()
        );
    }

    #[test]
    fn decompress_lzma_file_to_path_installs_payload_bytes() {
        let temp = tempfile::tempdir().expect("temp dir");
        let cab_path = temp.path().join("payload.cab");
        let output_path = temp.path().join("S1Game/S1Data/DataCenter_Final_EUR.dat");
        let payload = b"patched datacenter bytes";
        fs::write(&cab_path, lzma_compress_bytes(payload)).expect("write cab fixture");

        decompress_lzma_file_to_path(&cab_path, &output_path).expect("decompress cab");

        assert_eq!(fs::read(output_path).expect("read output"), payload);
    }

    #[test]
    fn decompress_lzma_file_to_path_with_limit_rejects_oversized_output() {
        let temp = tempfile::tempdir().expect("temp dir");
        let cab_path = temp.path().join("payload.cab");
        let output_path = temp.path().join("payload.bin");
        fs::write(&cab_path, lzma_compress_bytes(b"too large")).expect("write cab fixture");

        let err = decompress_lzma_file_to_path_with_limit(&cab_path, &output_path, Some(3))
            .expect_err("oversized decompressed output should fail");

        assert!(err.contains("exceeds 3 byte limit"));
    }

    #[test]
    fn validate_v100_db_cab_size_rejects_over_limit() {
        assert!(validate_v100_db_cab_size(MAX_V100_DB_CAB_BYTES).is_ok());
        assert!(validate_v100_db_cab_size(MAX_V100_DB_CAB_BYTES + 1).is_err());
    }

    #[tokio::test]
    async fn v100_static_patch_protocol_fetches_plans_and_installs_payload_e2e() {
        let temp = tempfile::tempdir().expect("temp dir");
        let http_root = temp.path().join("http");
        let patch_root = http_root.join("public/patch");
        let game_root = temp.path().join("game");
        fs::create_dir_all(patch_root.join("db")).expect("db dir");
        fs::create_dir_all(patch_root.join("patch")).expect("patch dir");
        fs::create_dir_all(game_root.join("S1Game/S1Data")).expect("game dir");
        fs::write(
            game_root.join("S1Game/S1Data/DataCenter_Final_EUR.dat"),
            b"stale datacenter bytes",
        )
        .expect("old game file");

        let payload = b"fresh datacenter bytes";
        let patch_cab = lzma_compress_bytes(payload);
        fs::write(patch_root.join("patch/1-1.cab"), &patch_cab).expect("patch cab");

        let db_path = temp.path().join("server.db");
        let conn = Connection::open(&db_path).expect("sqlite db");
        create_v100_schema(&conn);
        conn.execute(
            "INSERT INTO file_info (id, unique_path, path, property) VALUES (1, ?1, ?2, 0)",
            [
                "s1game\\s1data\\datacenter_final_eur.dat",
                "S1Game\\S1Data\\DataCenter_Final_EUR.dat",
            ],
        )
        .expect("file_info insert");
        conn.execute(
            "INSERT INTO file_version (id, version, size, hash) VALUES (1, 1, ?1, ?2)",
            (payload.len() as i64, md5_hex(payload)),
        )
        .expect("file_version insert");
        conn.execute(
            "INSERT INTO file_size (id, org_ver, new_ver, size) VALUES (1, -1, 1, ?1)",
            [patch_cab.len() as i64],
        )
        .expect("file_size insert");
        drop(conn);
        let db_bytes = fs::read(&db_path).expect("read db");
        fs::write(
            patch_root.join("db/server.db.1.cab"),
            lzma_compress_bytes(&db_bytes),
        )
        .expect("db cab");
        fs::write(
            patch_root.join("version.ini"),
            "[Download]\nVersion=1\nDL root=patch\nDB file=db/server.db.1.cab\n",
        )
        .expect("version.ini");

        let patch_base_url = serve_static_patch_fixture(http_root, 3);
        let client = reqwest::Client::new();
        let version_ini = client
            .get(join_patch_url(&patch_base_url, "version.ini"))
            .send()
            .await
            .expect("fetch version.ini")
            .error_for_status()
            .expect("version.ini status")
            .text()
            .await
            .expect("version.ini body");
        let manifest = parse_v100_version_ini(&version_ini).expect("manifest");
        let db_url = join_patch_url(&patch_base_url, &manifest.db_file);
        validate_v100_patch_url(&db_url, &patch_base_url).expect("db URL allowed");

        let db_cab_bytes = client
            .get(db_url)
            .send()
            .await
            .expect("fetch db cab")
            .error_for_status()
            .expect("db cab status")
            .bytes()
            .await
            .expect("db cab body");
        let staged_db_cab = game_root.join("$Patch/v100/db/server.db.1.cab");
        fs::create_dir_all(staged_db_cab.parent().expect("db cab parent")).expect("stage db dir");
        fs::write(&staged_db_cab, db_cab_bytes).expect("stage db cab");
        let staged_db = game_root.join("$Patch/v100/db/server.db.1");
        decompress_lzma_file_to_path(&staged_db_cab, &staged_db).expect("decompress db cab");

        let files = plan_v100_updates_from_db(&manifest, &patch_base_url, &staged_db, &game_root)
            .expect("plan update");
        assert_eq!(files.len(), 1);
        let planned = &files[0];
        assert_eq!(planned.install_mode, FileInstallMode::LzmaCab);
        assert_eq!(planned.hash_algorithm, FileHashAlgorithm::Md5);
        assert_eq!(planned.size, patch_cab.len() as u64);
        assert_eq!(planned.output_size, Some(payload.len() as u64));

        let patch_cab_bytes = client
            .get(&planned.url)
            .send()
            .await
            .expect("fetch patch cab")
            .error_for_status()
            .expect("patch cab status")
            .bytes()
            .await
            .expect("patch cab body");
        let staged_patch_cab =
            game_root.join(planned.download_path.as_deref().expect("download path"));
        fs::create_dir_all(staged_patch_cab.parent().expect("patch cab parent"))
            .expect("stage patch dir");
        fs::write(&staged_patch_cab, patch_cab_bytes).expect("stage patch cab");
        let final_path = game_root.join(&planned.path);
        decompress_lzma_file_to_path(&staged_patch_cab, &final_path).expect("install patch cab");

        assert_eq!(fs::read(&final_path).expect("installed bytes"), payload);
        assert_eq!(
            md5_file_hex(&final_path).expect("installed md5"),
            planned.hash
        );
    }

    #[tokio::test]
    #[ignore = "hits the external v100 patch test server by IP"]
    async fn v100_static_patch_protocol_fetches_test_server_ip_and_installs_payload_e2e() {
        let temp = tempfile::tempdir().expect("temp dir");
        let game_root = temp.path().join("game");
        let patch_base_url = "http://157.90.107.2:8090/public/patch";
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .expect("http client");

        let version_ini = client
            .get(join_patch_url(patch_base_url, "version.ini"))
            .send()
            .await
            .expect("fetch real version.ini")
            .error_for_status()
            .expect("real version.ini status")
            .text()
            .await
            .expect("real version.ini body");
        let manifest = parse_v100_version_ini(&version_ini).expect("real manifest");
        let db_url = join_patch_url(patch_base_url, &manifest.db_file);
        validate_v100_patch_url(&db_url, patch_base_url).expect("real db URL allowed");

        let db_cab_bytes = client
            .get(db_url)
            .send()
            .await
            .expect("fetch real db cab")
            .error_for_status()
            .expect("real db cab status")
            .bytes()
            .await
            .expect("real db cab body");
        validate_v100_db_cab_size(db_cab_bytes.len()).expect("real db cab size limit");

        let staged_db_cab = game_root.join("$Patch/v100/db/server.db.1.cab");
        fs::create_dir_all(staged_db_cab.parent().expect("real db cab parent"))
            .expect("stage real db dir");
        fs::write(&staged_db_cab, db_cab_bytes).expect("stage real db cab");
        let staged_db = game_root.join("$Patch/v100/db/server.db.1");
        decompress_lzma_file_to_path_with_limit(
            &staged_db_cab,
            &staged_db,
            Some(MAX_V100_DB_BYTES),
        )
        .expect("decompress real db cab");

        let files = plan_v100_updates_from_db(&manifest, patch_base_url, &staged_db, &game_root)
            .expect("real plan update");
        let planned = files
            .first()
            .expect("real server planned at least one file");
        assert_eq!(planned.install_mode, FileInstallMode::LzmaCab);
        assert_eq!(planned.hash_algorithm, FileHashAlgorithm::Md5);
        validate_v100_patch_url(&planned.url, patch_base_url).expect("real patch URL allowed");

        let patch_response = client
            .get(&planned.url)
            .send()
            .await
            .expect("fetch real patch cab")
            .error_for_status()
            .expect("real patch cab status");
        if let Some(content_length) = patch_response.content_length() {
            assert_eq!(content_length, planned.size);
        }
        let patch_cab_bytes = patch_response.bytes().await.expect("real patch cab body");
        assert_eq!(patch_cab_bytes.len() as u64, planned.size);

        let staged_patch_cab =
            game_root.join(planned.download_path.as_deref().expect("download path"));
        fs::create_dir_all(staged_patch_cab.parent().expect("real patch cab parent"))
            .expect("stage real patch dir");
        fs::write(&staged_patch_cab, patch_cab_bytes).expect("stage real patch cab");
        let final_path = game_root.join(&planned.path);
        decompress_lzma_file_to_path_with_limit(
            &staged_patch_cab,
            &final_path,
            planned.output_size,
        )
        .expect("install real patch cab");

        if let Some(expected_size) = planned.output_size {
            assert_eq!(
                fs::metadata(&final_path)
                    .expect("real installed metadata")
                    .len(),
                expected_size
            );
        }
        assert_eq!(
            md5_file_hex(&final_path).expect("real installed md5"),
            planned.hash
        );
    }

    #[test]
    fn v100_plan_uses_lzma_cab_download_for_changed_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let game_root = temp.path().join("game");
        let db_path = temp.path().join("server.db");
        fs::create_dir_all(game_root.join("S1Game/S1Data")).expect("game dirs");
        fs::write(
            game_root.join("S1Game/S1Data/DataCenter_Final_EUR.dat"),
            b"old",
        )
        .expect("old file");

        let conn = Connection::open(&db_path).expect("sqlite db");
        create_v100_schema(&conn);
        conn.execute(
            "INSERT INTO file_info (id, unique_path, path, property) VALUES (1, ?1, ?2, 0)",
            [
                "s1game\\s1data\\datacenter_final_eur.dat",
                "S1Game\\S1Data\\DataCenter_Final_EUR.dat",
            ],
        )
        .expect("file_info insert");
        conn.execute(
            "INSERT INTO file_version (id, version, size, hash) VALUES (1, 1, 3, ?1)",
            [md5_hex(b"new")],
        )
        .expect("file_version insert");
        conn.execute(
            "INSERT INTO file_size (id, org_ver, new_ver, size) VALUES (1, -1, 1, 42)",
            [],
        )
        .expect("file_size insert");
        drop(conn);

        let manifest = V100VersionManifest {
            version: 1,
            dl_root: "patch".to_string(),
            db_file: "db/server.db.1.cab".to_string(),
        };

        let files = plan_v100_updates_from_db(
            &manifest,
            "http://127.0.0.1:8090/public/patch",
            &db_path,
            &game_root,
        )
        .expect("v100 plan");

        assert_eq!(files.len(), 1);
        let planned = &files[0];
        assert_eq!(planned.path, "S1Game/S1Data/DataCenter_Final_EUR.dat");
        assert_eq!(
            planned.url,
            "http://127.0.0.1:8090/public/patch/patch/1-1.cab"
        );
        assert_eq!(planned.size, 42);
        assert_eq!(planned.hash, md5_hex(b"new"));
        assert_eq!(planned.hash_algorithm, FileHashAlgorithm::Md5);
        assert_eq!(planned.install_mode, FileInstallMode::LzmaCab);
        assert_eq!(
            planned.download_path.as_deref(),
            Some("$Patch/v100/patch/1-1.cab")
        );
        assert_eq!(planned.output_size, Some(3));
    }

    #[test]
    fn v100_plan_rejects_oversized_patch_cab_metadata() {
        let temp = tempfile::tempdir().expect("temp dir");
        let game_root = temp.path().join("game");
        let db_path = temp.path().join("server.db");
        let conn = Connection::open(&db_path).expect("sqlite db");
        create_v100_schema(&conn);
        conn.execute(
            "INSERT INTO file_info (id, unique_path, path, property) VALUES (1, ?1, ?2, 0)",
            ["oversized.bin", "oversized.bin"],
        )
        .expect("file_info insert");
        conn.execute(
            "INSERT INTO file_version (id, version, size, hash) VALUES (1, 1, 1, ?1)",
            [md5_hex(b"x")],
        )
        .expect("file_version insert");
        conn.execute(
            "INSERT INTO file_size (id, org_ver, new_ver, size) VALUES (1, -1, 1, ?1)",
            [(MAX_V100_PATCH_CAB_BYTES + 1) as i64],
        )
        .expect("file_size insert");
        drop(conn);

        let manifest = V100VersionManifest {
            version: 1,
            dl_root: "patch".to_string(),
            db_file: "db/server.db.1.cab".to_string(),
        };

        let err = plan_v100_updates_from_db(
            &manifest,
            "http://127.0.0.1:8090/public/patch",
            &db_path,
            &game_root,
        )
        .expect_err("oversized patch CAB metadata should fail");

        assert!(err.contains("v100 patch CAB for file id 1 is too large"));
    }

    #[test]
    fn v100_plan_rejects_oversized_patch_output_metadata() {
        let temp = tempfile::tempdir().expect("temp dir");
        let game_root = temp.path().join("game");
        let db_path = temp.path().join("server.db");
        let conn = Connection::open(&db_path).expect("sqlite db");
        create_v100_schema(&conn);
        conn.execute(
            "INSERT INTO file_info (id, unique_path, path, property) VALUES (1, ?1, ?2, 0)",
            ["oversized.bin", "oversized.bin"],
        )
        .expect("file_info insert");
        conn.execute(
            "INSERT INTO file_version (id, version, size, hash) VALUES (1, 1, ?1, ?2)",
            ((MAX_V100_PATCH_OUTPUT_BYTES + 1) as i64, md5_hex(b"x")),
        )
        .expect("file_version insert");
        drop(conn);

        let manifest = V100VersionManifest {
            version: 1,
            dl_root: "patch".to_string(),
            db_file: "db/server.db.1.cab".to_string(),
        };

        let err = plan_v100_updates_from_db(
            &manifest,
            "http://127.0.0.1:8090/public/patch",
            &db_path,
            &game_root,
        )
        .expect_err("oversized patch output metadata should fail");

        assert!(err.contains("v100 patch output for file id 1 is too large"));
    }

    fn create_v100_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
CREATE TABLE file_info (
    id integer,
    unique_path text UNIQUE,
    path text,
    property integer,
    PRIMARY KEY(id)
);
CREATE TABLE file_version (
    id integer,
    version integer,
    size integer,
    hash text
);
CREATE TABLE file_size (
    id integer,
    org_ver integer,
    new_ver integer,
    size integer
);
"#,
        )
        .expect("schema");
    }

    fn lzma_compress_bytes(payload: &[u8]) -> Vec<u8> {
        let mut compressed = Vec::new();
        lzma_rs::lzma_compress(&mut Cursor::new(payload), &mut compressed)
            .expect("compress fixture");
        compressed
    }

    fn serve_static_patch_fixture(root: PathBuf, request_count: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        let base_url = format!(
            "http://{}/public/patch",
            listener.local_addr().expect("addr")
        );

        std::thread::spawn(move || {
            for _ in 0..request_count {
                let (mut stream, _) = listener.accept().expect("accept fixture request");
                serve_static_file(&root, &mut stream);
            }
        });

        base_url
    }

    fn serve_static_file(root: &Path, stream: &mut TcpStream) {
        let mut request = [0u8; 2048];
        let read = stream.read(&mut request).expect("read request");
        let request_line = String::from_utf8_lossy(&request[..read])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .trim_start_matches('/')
            .split('?')
            .next()
            .unwrap_or_default();
        let file_path = root.join(path);

        match fs::read(file_path) {
            Ok(body) => write_http_response(stream, 200, "OK", &body),
            Err(_) => write_http_response(stream, 404, "Not Found", b"not found"),
        }
    }

    fn write_http_response(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) {
        write!(
            stream,
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            status,
            reason,
            body.len()
        )
        .expect("write response header");
        stream.write_all(body).expect("write response body");
    }
}
