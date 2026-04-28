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
#   3) Or set env vars TAURI_PRIVATE_KEY / TAURI_KEY_PASSWORD (legacy v1 names) — this script
#      forwards them to the v2 names TAURI_SIGNING_PRIVATE_KEY /
#      TAURI_SIGNING_PRIVATE_KEY_PASSWORD that the v2 CLI reads.
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

# -- Tauri CLI Check (Optional if using npm)
if (-not $cargoTauriCheck) {
    Write-Host "[Info] Global cargo-tauri not found, will rely on npm script."
} else {
    Write-Host "[OK] Cargo Tauri CLI erkannt" -ForegroundColor $success
}

# -- NSIS
$makensisCheck = Get-Command makensis -ErrorAction SilentlyContinue

# Potential NSIS paths
$potentialPaths = @(
    "${env:ProgramFiles(x86)}\NSIS\makensis.exe",
    "${env:ProgramFiles(x86)}\NSIS\Bin\makensis.exe",
    "${env:ProgramFiles}\NSIS\makensis.exe",
    "${env:ProgramFiles}\NSIS\Bin\makensis.exe",
    "C:\ProgramData\chocolatey\bin\makensis.exe",
    "C:\ProgramData\chocolatey\lib\nsis\tools\makensis.exe"
)
$chocolateyNsisRoot = "C:\ProgramData\chocolatey\lib\nsis"

$foundNsis = $false

if ($makensisCheck) {
    $foundNsis = $true
} else {
    foreach ($path in $potentialPaths) {
        if (Test-Path $path) {
            $foundNsis = $true
            # Add to PATH for current session so tauri cli can find it
            $nsisDir = Split-Path -Parent $path
            $env:Path += ";$nsisDir"
            break
        }
    }

    if (-not $foundNsis -and (Test-Path $chocolateyNsisRoot)) {
        $chocolateyNsis = Get-ChildItem -LiteralPath $chocolateyNsisRoot -Filter "makensis.exe" -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($chocolateyNsis) {
            $foundNsis = $true
            $env:Path += ";$($chocolateyNsis.DirectoryName)"
        }
    }
}

if (-not $foundNsis) {
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

# Helper: read values from .env files
function Get-DotEnvValue {
    param(
        [string]$FilePath,
        [string]$Key
    )
    if (-Not (Test-Path $FilePath)) { return $null }
    try {
        foreach ($line in Get-Content -LiteralPath $FilePath) {
            if ($null -eq $line -or $line.Trim().StartsWith('#') -or [string]::IsNullOrWhiteSpace($line)) { continue }
            $parts = $line -split '=', 2
            if ($parts.Length -ne 2) { continue }
            $k = $parts[0].Trim()
            if ($k -ne $Key) { continue }
            $v = $parts[1].Trim()
            # Strip surrounding quotes if present
            if (($v.StartsWith('"') -and $v.EndsWith('"')) -or ($v.StartsWith("'") -and $v.EndsWith("'"))) {
                $v = $v.Substring(1, $v.Length - 2)
            }
            return $v
        }
    } catch {}
    return $null
}

# Potential .env locations (repo root, app root, src-tauri)
$dotenvCandidates = @(
    (Join-Path $PSScriptRoot ".env"),
    (Join-Path $projectPath ".env"),
    (Join-Path (Join-Path $projectPath "src-tauri") ".env")
)

# Precedence: Inline > Files > Existing Env
if ($null -ne $PrivateKeyInline -and -not [string]::IsNullOrWhiteSpace($PrivateKeyInline)) {
    $resolvedKey = $PrivateKeyInline
} elseif (Test-Path (Join-Path $PSScriptRoot "tauri_private_key.txt")) {
    $resolvedKey = Get-Content (Join-Path $PSScriptRoot "tauri_private_key.txt") -Raw
} elseif (-not $resolvedKey) {
    foreach ($envFile in $dotenvCandidates) {
        $val = Get-DotEnvValue -FilePath $envFile -Key 'TAURI_PRIVATE_KEY'
        if ($val) { $resolvedKey = $val; break }
    }
} elseif ($env:TAURI_PRIVATE_KEY) {
    $resolvedKey = $env:TAURI_PRIVATE_KEY
}

if ($null -ne $PrivateKeyPasswordInline -and -not [string]::IsNullOrWhiteSpace($PrivateKeyPasswordInline)) {
    $resolvedPwd = $PrivateKeyPasswordInline
} elseif (Test-Path (Join-Path $PSScriptRoot "tauri_private_key_password.txt")) {
    $resolvedPwd = Get-Content (Join-Path $PSScriptRoot "tauri_private_key_password.txt") -Raw
} elseif (-not $resolvedPwd) {
    foreach ($envFile in $dotenvCandidates) {
        $val = Get-DotEnvValue -FilePath $envFile -Key 'TAURI_KEY_PASSWORD'
        if ($val) { $resolvedPwd = $val; break }
    }
} elseif ($env:TAURI_KEY_PASSWORD) {
    $resolvedPwd = $env:TAURI_KEY_PASSWORD
}

if ($resolvedKey -and -not [string]::IsNullOrWhiteSpace($resolvedKey)) {
    # Tauri v2 reads TAURI_SIGNING_PRIVATE_KEY(_PASSWORD); set both the v2
    # names and the legacy v1 names so either CLI version finds the key.
    $env:TAURI_SIGNING_PRIVATE_KEY = $resolvedKey.Trim()
    $env:TAURI_PRIVATE_KEY = $resolvedKey.Trim()
    # Always set password (even empty string) — Tauri requires it to decrypt the key
    $pwdTrim = if ($resolvedPwd) { $resolvedPwd.Trim() } else { "" }
    $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $pwdTrim
    $env:TAURI_KEY_PASSWORD = $pwdTrim
    Write-Host "`n[OK] Updater auto-signing aktiviert (TAURI_SIGNING_PRIVATE_KEY gesetzt)" -ForegroundColor $success
} else {
    Write-Host "`n[!] Kein privater Signierschluessel gefunden - Bundler zeigt ggf. Hinweis, Artefakte werden trotzdem erstellt." -ForegroundColor $warn
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

# -- Install Node Dependencies
Write-Host "`nInstalliere Node-Abhaengigkeiten..." -ForegroundColor $warn
npm install
if ($LASTEXITCODE -ne 0) {
    Write-Host "[!] npm install fehlgeschlagen" -ForegroundColor $fail
    exit 1
}

# -- Tauri Build starten (via NPM for speed)
Write-Host "`nBaue Projekt via: npm run tauri build" -ForegroundColor $warn
npm run tauri build
if ($LASTEXITCODE -ne 0) {
    # Fallback to cargo tauri if npm fails?
    Write-Host "[!] npm run tauri build failed. Trying cargo tauri..." -ForegroundColor $warn
    if (-not $cargoTauriCheck) {
        Write-Host "Installing cargo-tauri..."
        cargo install --locked tauri-cli@^2
    }
    cargo tauri build
}

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
