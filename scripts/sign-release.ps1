# Builds the signed ODM release: run from anywhere with
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\sign-release.ps1
# The update key password is asked here (hidden), checked against the key in a few seconds
# (3 tries), and kept only in this process's environment for the build. It is cleared
# afterwards and never written down.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$key = Join-Path $env:USERPROFILE '.tauri\odm-updater.key'
if (-not (Test-Path $key)) {
    Write-Host "No signing key at $key. See 'Releasing' in README.md." -ForegroundColor Red
    exit 1
}
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = $key
$probe = Join-Path $env:TEMP 'odm-sign-probe.txt'
Set-Content -Path $probe -Value 'probe' -Encoding ascii

$ok = $false
for ($try = 1; $try -le 3 -and -not $ok; $try++) {
    $secure = Read-Host "ODM update key password (try $try of 3)" -AsSecureString
    $ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {
        $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr)
    } finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr)
    }
    Write-Host ("  (that was {0} characters)" -f $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD.Length)
    Push-Location app
    # Through cmd: Windows PowerShell treats pnpm's stderr chatter as a terminating error.
    cmd /c "pnpm tauri signer sign --private-key-path `"$key`" `"$probe`" >nul 2>nul"
    $ok = ($LASTEXITCODE -eq 0)
    Pop-Location
    Remove-Item "$probe.sig" -ErrorAction SilentlyContinue
    if ($ok) { Write-Host "  Password is right." -ForegroundColor Green } else { Write-Host "  Wrong password for that key." -ForegroundColor Yellow }
}
Remove-Item $probe -ErrorAction SilentlyContinue

if (-not $ok) {
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
    Write-Host "`nSIGNED BUILD NOT STARTED (password not accepted)" -ForegroundColor Red
    exit 1
}
try {
    node scripts\release.mjs --publish-ready
    $code = $LASTEXITCODE
} finally {
    Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue
}
if ($code -eq 0) { Write-Host "`nSIGNED BUILD DONE" -ForegroundColor Green } else { Write-Host "`nSIGNED BUILD FAILED (exit $code)" -ForegroundColor Red }
exit $code
