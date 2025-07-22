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
$licenseContent = @"
TERA Germany Launcher – Nutzungsbedingungen
===========================================

TERA Germany Launcher – Endbenutzer-Lizenzvereinbarung (EULA)
Copyright © 2025 Crazy-Esports
Alle Rechte vorbehalten

1. Zweck und Nutzung
Dieser Launcher ist ausschließlich für den privaten und nicht-kommerziellen Zugriff auf den Fan-Server TERA Germany vorgesehen.
Jegliche Nutzung außerhalb dieses Zwecks, insbesondere kommerzielle Nutzung, ist untersagt.

2. Eigentum und geistiges Eigentum
Der Launcher, einschließlich aller Inhalte, Designs, Logos und Quellcodes, ist Eigentum von Crazy-Esports und unterliegt dem Urheberrecht.

3. Veränderungen und Weitergabe
Es ist untersagt, diesen Launcher zu verändern, zu dekompilieren, zurückzuentwickeln, zu disassemblieren oder auf andere Weise zu analysieren oder weiterzugeben, es sei denn, Crazy-Esports hat dies ausdrücklich schriftlich genehmigt.

4. Haftungsausschluss
Crazy-Esports übernimmt keine Haftung für:

Schäden oder Datenverluste, die durch die Nutzung des Launchers entstehen,

eventuelle Inkompatibilitäten mit Systemen,

externe Inhalte, die über den Launcher geladen werden.

Die Nutzung erfolgt auf eigenes Risiko.

5. Datenschutz und Datenverarbeitung
Der Launcher kann bestimmte technische Daten erheben (z. B. IP-Adresse, Clientversion, Verbindungszeitpunkte), die zur Sicherstellung der Funktion und Sicherheit erforderlich sind.
Diese Daten werden gemäß der geltenden Datenschutz-Grundverordnung (DSGVO) verarbeitet. Weitere Informationen findest du in unserer Datenschutzerklärung.

6. Beziehung zu Dritten
TERA Germany ist ein nicht-kommerzielles Fanprojekt. Es besteht keine offizielle Verbindung zu KRAFTON, Bluehole oder anderen Rechteinhabern von TERA.
Alle verwendeten Namen, Marken und Inhalte sind Eigentum der jeweiligen Rechteinhaber.

7. Geltendes Recht
Diese Vereinbarung unterliegt dem Recht der Bundesrepublik Deutschland unter Berücksichtigung der europäischen Datenschutzrichtlinien.

Mit dem Klicken auf „Weiter“ bestätigst du, dass du diese Bedingungen gelesen und akzeptiert hast.


---

TERA Germany Launcher – Terms of Use
=====================================

TERA Germany Launcher – End User License Agreement (EULA)
Copyright © 2025 Crazy-Esports
All rights reserved

1. Purpose and Use
This launcher is intended solely for private and non-commercial access to the fan-based TERA Germany server.
Any use beyond this purpose, especially commercial use, is strictly prohibited.

2. Ownership and Intellectual Property
The launcher, including all assets, designs, logos, and source code, is the property of Crazy-Esports and protected under copyright laws.

3. Modifications and Distribution
You may not modify, decompile, reverse engineer, disassemble, or otherwise analyze or redistribute this launcher unless explicitly authorized in writing by Crazy-Esports.

4. Disclaimer of Liability
Crazy-Esports assumes no liability for:

damages or data loss resulting from the use of the launcher,

system incompatibilities,

external content loaded through the launcher.

Use is at your own risk.

5. Data Protection and Processing
This launcher may collect technical data (e.g., IP address, client version, timestamps) necessary for operation and security.
Such data is processed in compliance with the General Data Protection Regulation (GDPR). For more, please refer to our Privacy Policy.

6. Relation to Third Parties
TERA Germany is a non-commercial fan project.
There is no official affiliation with KRAFTON, Bluehole, or any other rights holders of TERA.
All referenced names, brands, and assets are property of their respective owners.

7. Governing Law
This agreement is governed by the laws of Germany, considering applicable European Union data protection regulations.

By clicking “Next”, you confirm that you have read and accepted these terms.

"@

Write-Host "`nSchreibe Lizenzdatei..."
$licenseContent | Out-File -FilePath $licenseFile -Encoding UTF8
Write-Host "✓ license.txt geschrieben" -ForegroundColor $success

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

