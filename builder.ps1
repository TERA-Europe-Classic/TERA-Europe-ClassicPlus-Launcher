# ===============================
# * TERA Germany Launcher Builder (All-in-One)
# ===============================
$projectPath = Join-Path $PSScriptRoot "teralaunch"
$nsisPath = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
$licenseFile = Join-Path $projectPath "license.txt"
$npmCheck = Get-Command npm -ErrorAction SilentlyContinue
$rustCheck = Get-Command rustc -ErrorAction SilentlyContinue
$cargoTauriCheck = Get-Command cargo-tauri -ErrorAction SilentlyContinue

# ===============================
# Optional: Updater signing configuration (private key)
# - You can either:
#   1) Paste base64-encoded private key into $PrivateKeyInline and password into $PrivateKeyPasswordInline, OR
#   2) Place files next to this script: tauri_private_key.txt and tauri_private_key_password.txt
#   3) Or set environment variables TAURI_PRIVATE_KEY and TAURI_KEY_PASSWORD before running the script
# NOTE: Do NOT commit real secrets to git. These values are optional; if none provided, build continues without auto-signing.
# ===============================
$PrivateKeyInline = $null          # e.g. @"BASE64_KEY_HERE"@
$PrivateKeyPasswordInline = $null  # e.g. "myPassword" (if encrypted)

# Farben
$success = "Green"
$warn = "Yellow"
$fail = "Red"

Write-Host ""
Write-Host "Starte vollstaendigen Build fuer den TERA Germany Launcher..." -ForegroundColor $success

# -- Node.js
if (-not $npmCheck) {
    Write-Host "`n[!] Node.js nicht gefunden - oeffne Downloadseite..." -ForegroundColor $warn
    Start-Process "https://nodejs.org"
    exit 1
} else {
    Write-Host "[OK] Node.js erkannt" -ForegroundColor $success
}

# -- Rust
if (-not $rustCheck) {
    Write-Host "`n[!] Rust nicht gefunden - oeffne rustup-Installer..." -ForegroundColor $warn
    Start-Process "https://rustup.rs"
    exit 1
} else {
    Write-Host "[OK] Rust installiert" -ForegroundColor $success
}

# -- Tauri CLI (Cargo v1)
if (-not $cargoTauriCheck) {
    Write-Host "`n[!] Cargo Tauri CLI v1 fehlt - wird installiert..." -ForegroundColor $warn
    cargo install --locked tauri-cli@^1
} else {
    Write-Host "[OK] Cargo Tauri CLI erkannt" -ForegroundColor $success
}

# -- NSIS
if (-Not (Test-Path $nsisPath)) {
    Write-Host "`n[!] NSIS nicht gefunden!" -ForegroundColor $fail
    Write-Host "Bitte installiere NSIS: https://nsis.sourceforge.io/Download" -ForegroundColor $warn
    Start-Process "https://nsis.sourceforge.io/Download"
    exit 1
} else {
    Write-Host "[OK] NSIS vorhanden" -ForegroundColor $success
}

# -- Updater auto-signing (set env vars if available)
$resolvedKey = $null
$resolvedPwd = $null

# Precedence: Inline > Files > Existing Env
if ($null -ne $PrivateKeyInline -and -not [string]::IsNullOrWhiteSpace($PrivateKeyInline)) {
    $resolvedKey = $PrivateKeyInline
} elseif (Test-Path (Join-Path $PSScriptRoot "tauri_private_key.txt")) {
    $resolvedKey = Get-Content (Join-Path $PSScriptRoot "tauri_private_key.txt") -Raw
} elseif ($env:TAURI_PRIVATE_KEY) {
    $resolvedKey = $env:TAURI_PRIVATE_KEY
}

if ($null -ne $PrivateKeyPasswordInline -and -not [string]::IsNullOrWhiteSpace($PrivateKeyPasswordInline)) {
    $resolvedPwd = $PrivateKeyPasswordInline
} elseif (Test-Path (Join-Path $PSScriptRoot "tauri_private_key_password.txt")) {
    $resolvedPwd = Get-Content (Join-Path $PSScriptRoot "tauri_private_key_password.txt") -Raw
} elseif ($env:TAURI_KEY_PASSWORD) {
    $resolvedPwd = $env:TAURI_KEY_PASSWORD
}

if ($resolvedKey -and -not [string]::IsNullOrWhiteSpace($resolvedKey)) {
    $env:TAURI_PRIVATE_KEY = $resolvedKey.Trim()
    if ($resolvedPwd -and -not [string]::IsNullOrWhiteSpace($resolvedPwd)) {
        $env:TAURI_KEY_PASSWORD = $resolvedPwd.Trim()
    }
    Write-Host "\n[OK] Updater auto-signing aktiviert (TAURI_PRIVATE_KEY gesetzt)" -ForegroundColor $success
} else {
    Write-Host "\n[!] Kein privater Signierschluessel gefunden – Bundler zeigt ggf. Hinweis, Artefakte werden trotzdem erstellt." -ForegroundColor $warn
}

# -- Lizenzdatei schreiben (Deutsch/Englisch)
$licenseSource = Join-Path $PSScriptRoot "license.txt"
if (-Not (Test-Path $licenseSource)) {
    Write-Host "`n[!] license.txt nicht gefunden: $licenseSource" -ForegroundColor $fail
    exit 1
}

Write-Host "`nKopiere Lizenzdatei..."
Copy-Item -Path $licenseSource -Destination $licenseFile -Force
Write-Host "[OK] license.txt kopiert" -ForegroundColor $success

# -- Projektverzeichnis pruefen
if (-Not (Test-Path $projectPath)) {
    Write-Host "`n[!] Projektverzeichnis nicht gefunden: $projectPath" -ForegroundColor $fail
    exit 1
}
Set-Location -Path $projectPath
Write-Host "`nWechsle ins Projektverzeichnis: $projectPath" -ForegroundColor $success

# -- Tauri Build starten (Cargo v1 CLI)
Write-Host "`nBaue Projekt via: cargo tauri build" -ForegroundColor $warn
cargo tauri build

# -- Installer finden
$installerPath = Join-Path $projectPath "src-tauri\target\release\bundle\nsis"
$setupFiles = Get-ChildItem -Path $installerPath -Filter "*setup.exe"

if ($setupFiles.Count -gt 0) {
    $latestInstaller = $setupFiles | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Write-Host "`n[OK] Installer erfolgreich erstellt:" -ForegroundColor $success
    Write-Host "  $($latestInstaller.FullName)" -ForegroundColor $success
} else {
    Write-Host "`n[!] Kein Installer gefunden! Bitte tauri.conf.json pruefen." -ForegroundColor $fail
}

