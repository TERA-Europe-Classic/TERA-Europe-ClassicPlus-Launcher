use std::env;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use reqwest::redirect::Policy;
use sha2::{Digest, Sha256};

#[path = "../services/mods/catalog_audit.rs"]
mod catalog_audit;

const USAGE: &str = concat!(
    "gpk-catalog-audit\n",
    "\n",
    "Usage:\n",
    "  gpk-catalog-audit --catalog <catalog.json> --out <report.md> [--cache-dir <dir>] [--download-missing]\n",
    "\n",
    "Flags:\n",
    "  --catalog   Path to external-mod-catalog/catalog.json\n",
    "  --out       Markdown report path to write\n",
    "  --cache-dir Optional SHA-addressed GPK cache directory for header facts\n",
    "  --download-missing  Download missing cached GPKs before header audit\n",
    "  -h, --help  Show this help and exit\n"
);

#[derive(Debug)]
struct CliArgs {
    catalog: PathBuf,
    out: PathBuf,
    cache_dir: Option<PathBuf>,
    download_missing: bool,
}

#[tokio::main]
async fn main() {
    match parse_args(env::args_os().skip(1).collect()) {
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
        }
        Ok(ParseOutcome::Args(args)) => {
            if let Err(err) = run(args).await {
                eprintln!("{err}");
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("{err}\n\n{USAGE}");
            std::process::exit(1);
        }
    }
}

async fn run(args: CliArgs) -> Result<(), String> {
    let body = std::fs::read_to_string(&args.catalog)
        .map_err(|e| format!("Failed to read catalog {}: {e}", args.catalog.display()))?;
    let catalog: catalog_audit::AuditCatalog =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse catalog JSON: {e}"))?;
    if args.download_missing {
        let cache_dir = args
            .cache_dir
            .as_deref()
            .ok_or_else(|| "--download-missing requires --cache-dir".to_string())?;
        cache_missing_gpk_artifacts(&catalog, cache_dir).await?;
    } else if args.cache_dir.is_some() {
        validate_cacheable_catalog_sha_values(&catalog)?;
    }
    let rows = if let Some(cache_dir) = args.cache_dir.as_deref() {
        catalog_audit::audit_catalog_with_cached_headers(&catalog, cache_dir)
    } else {
        catalog_audit::audit_catalog(&catalog)
    };
    let report = catalog_audit::render_markdown_report(&catalog, &rows);

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create report directory {}: {e}",
                    parent.display()
                )
            })?;
        }
    }
    std::fs::write(&args.out, report)
        .map_err(|e| format!("Failed to write report {}: {e}", args.out.display()))?;
    println!("audit-rows: {}", rows.len());
    println!("report: {}", args.out.display());
    Ok(())
}

enum ParseOutcome {
    Help,
    Args(CliArgs),
}

fn parse_args(args: Vec<OsString>) -> Result<ParseOutcome, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.to_str(), Some("-h") | Some("--help")))
    {
        return Ok(ParseOutcome::Help);
    }

    let mut catalog = None;
    let mut out = None;
    let mut cache_dir = None;
    let mut download_missing = false;
    let mut iter = args.into_iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--catalog" => catalog = Some(parse_path_value("--catalog", &mut iter)?),
            "--out" => out = Some(parse_path_value("--out", &mut iter)?),
            "--cache-dir" => cache_dir = Some(parse_path_value("--cache-dir", &mut iter)?),
            "--download-missing" => download_missing = true,
            value if value.starts_with('-') => return Err(format!("Unknown flag: {value}")),
            value => return Err(format!("Unexpected positional argument: {value}")),
        }
    }

    Ok(ParseOutcome::Args(CliArgs {
        catalog: catalog.ok_or_else(|| "Missing required flag: --catalog".to_string())?,
        out: out.ok_or_else(|| "Missing required flag: --out".to_string())?,
        cache_dir,
        download_missing,
    }))
}

async fn cache_missing_gpk_artifacts(
    catalog: &catalog_audit::AuditCatalog,
    cache_dir: &std::path::Path,
) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir).map_err(|e| {
        format!(
            "Failed to create cache directory {}: {e}",
            cache_dir.display()
        )
    })?;
    let client = reqwest::Client::builder()
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.stop();
            }
            if is_trusted_download_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("TERA-Europe-ClassicPlus-Launcher/gpk-catalog-audit")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    for entry in &catalog.mods {
        if entry.kind != "gpk" || entry.sha256.trim().is_empty() {
            continue;
        }
        let sha256 = validated_sha256(entry)?;
        let cache_path = cache_dir.join(format!("{sha256}.gpk"));
        if cache_path.exists() {
            continue;
        }
        validate_download_url(entry)?;
        let bytes = client
            .get(&entry.download_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download {}: {e}", entry.id))?
            .error_for_status()
            .map_err(|e| format!("Failed to download {}: {e}", entry.id))?
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download body for {}: {e}", entry.id))?;
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != sha256 {
            return Err(format!(
                "SHA-256 mismatch for {}: expected {}, got {}",
                entry.id, sha256, actual
            ));
        }
        let tmp_path = cache_dir.join(format!("{sha256}.{}.gpk.tmp", std::process::id()));
        std::fs::write(&tmp_path, &bytes).map_err(|e| {
            format!(
                "Failed to write cache temp file {}: {e}",
                tmp_path.display()
            )
        })?;
        std::fs::rename(&tmp_path, &cache_path).map_err(|e| {
            format!(
                "Failed to promote cache file {} to {}: {e}",
                tmp_path.display(),
                cache_path.display()
            )
        })?;
    }

    Ok(())
}

fn validated_sha256(entry: &catalog_audit::AuditCatalogEntry) -> Result<String, String> {
    catalog_audit::validated_sha256(entry).ok_or_else(|| {
        format!(
            "Invalid SHA-256 for {}: expected 64 hexadecimal characters",
            entry.id
        )
    })
}

fn validate_cacheable_catalog_sha_values(
    catalog: &catalog_audit::AuditCatalog,
) -> Result<(), String> {
    for entry in &catalog.mods {
        if entry.kind == "gpk" && !entry.sha256.trim().is_empty() {
            validated_sha256(entry)?;
        }
    }
    Ok(())
}

fn validate_download_url(entry: &catalog_audit::AuditCatalogEntry) -> Result<(), String> {
    let url = reqwest::Url::parse(&entry.download_url)
        .map_err(|e| format!("Invalid download URL for {}: {e}", entry.id))?;
    if is_trusted_download_url(&url) {
        Ok(())
    } else {
        Err(format!(
            "Untrusted download URL for {}: expected HTTPS GitHub-hosted artifact URL",
            entry.id
        ))
    }
}

fn is_trusted_download_url(url: &reqwest::Url) -> bool {
    let scheme = url.scheme();
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let github_artifact_url = scheme == "https"
        && matches!(
            host.as_str(),
            "github.com"
                | "raw.githubusercontent.com"
                | "objects.githubusercontent.com"
                | "github-releases.githubusercontent.com"
                | "release-assets.githubusercontent.com"
        );
    let test_loopback_url = env::var_os("GPK_CATALOG_AUDIT_ALLOW_LOOPBACK").is_some()
        && scheme == "http"
        && matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
    github_artifact_url || test_loopback_url
}

fn parse_path_value(
    flag: &str,
    iter: &mut std::iter::Peekable<std::vec::IntoIter<OsString>>,
) -> Result<PathBuf, String> {
    let value = iter
        .next()
        .ok_or_else(|| format!("Missing value for {flag}"))?;
    if is_flag_token(&value) {
        return Err(format!("Missing value for {flag}"));
    }
    if value.is_empty() {
        return Err(format!("Empty value for {flag}"));
    }
    Ok(PathBuf::from(value))
}

fn is_flag_token(value: &OsStr) -> bool {
    matches!(value.to_str(), Some(text) if text.starts_with('-'))
}
