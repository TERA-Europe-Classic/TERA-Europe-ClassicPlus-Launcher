# ===============================
# 💥 TERA Germany Launcher Builder (All-in-One)
# ===============================
$projectPath = "C:\TERALauncher\Crimson\TERA-Launcher\teralaunch"
$nsisPath = "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
$licenseFile = Join-Path $projectPath "license.txt"
$npmCheck = Get-Command npm -ErrorAction SilentlyContinue
$rustCheck = Get-Command rustc -ErrorAction SilentlyContinue
$tauriCheck = Get-Command tauri -ErrorAction SilentlyContinue

# Farben
$success = "Green"
$warn = "Yellow"
$fail = "Red"

Write-Host ""
Write-Host "Starte vollständigen Build für den TERA Germany Launcher..." -ForegroundColor $success

# -- Node.js
if (-not $npmCheck) {
    Write-Host "`n[!] Node.js nicht gefunden – öffne Downloadseite..." -ForegroundColor $warn
    Start-Process "https://nodejs.org"
    exit 1
} else {
    Write-Host "✓ Node.js erkannt" -ForegroundColor $success
}

# -- Rust
if (-not $rustCheck) {
    Write-Host "`n[!] Rust nicht gefunden – öffne rustup-Installer..." -ForegroundColor $warn
    Start-Process "https://rustup.rs"
    exit 1
} else {
    Write-Host "✓ Rust installiert" -ForegroundColor $success
}

# -- Tauri CLI
if (-not $tauriCheck) {
    Write-Host "`n[!] Tauri CLI fehlt – wird installiert..." -ForegroundColor $warn
    npm install -g @tauri-apps/cli
} else {
    Write-Host "✓ Tauri CLI erkannt" -ForegroundColor $success
}

# -- NSIS
if (-Not (Test-Path $nsisPath)) {
    Write-Host "`n[!] NSIS nicht gefunden!" -ForegroundColor $fail
    Write-Host "Bitte installiere NSIS: https://nsis.sourceforge.io/Download" -ForegroundColor $warn
    Start-Process "https://nsis.sourceforge.io/Download"
    exit 1
} else {
    Write-Host "✓ NSIS vorhanden" -ForegroundColor $success
}

# -- Lizenzdatei schreiben (Deutsch/Englisch)
$licenseSource = Join-Path $PSScriptRoot "license.txt"
if (-Not (Test-Path $licenseSource)) {
    Write-Host "`n[!] license.txt nicht gefunden: $licenseSource" -ForegroundColor $fail
    exit 1
}

Write-Host "`nKopiere Lizenzdatei..."
Copy-Item -Path $licenseSource -Destination $licenseFile -Force
Write-Host "✓ license.txt kopiert" -ForegroundColor $success

# -- Projektverzeichnis prüfen
if (-Not (Test-Path $projectPath)) {
    Write-Host "`n[!] Projektverzeichnis nicht gefunden: $projectPath" -ForegroundColor $fail
    exit 1
}
Set-Location -Path $projectPath
Write-Host "`nWechsle ins Projektverzeichnis: $projectPath" -ForegroundColor $success

# -- Tauri Build starten
Write-Host "`nBaue Projekt via: npm run tauri build" -ForegroundColor $warn
npm run tauri build

# -- Installer finden
$installerPath = Join-Path $projectPath "src-tauri\target\release\bundle\nsis"
$setupFiles = Get-ChildItem -Path $installerPath -Filter "*setup.exe"

if ($setupFiles.Count -gt 0) {
    $latestInstaller = $setupFiles | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Write-Host "`n✓ Installer erfolgreich erstellt:" -ForegroundColor $success
    Write-Host "  $($latestInstaller.FullName)" -ForegroundColor $success
} else {
    Write-Host "`n[!] Kein Installer gefunden! Bitte tauri.conf.json prüfen." -ForegroundColor $fail
}

