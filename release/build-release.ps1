param(
    [string]$Repo = $env:FERROVOX_UPDATE_REPO,
    [string]$KeyFile = (Join-Path $HOME ".ferrovox-release\signing-key.txt")
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($env:GITHUB_REF_NAME -and $env:GITHUB_REF_NAME -ne "v$version") {
    throw "Tag $env:GITHUB_REF_NAME passt nicht zur Version $version in Cargo.toml"
}
Write-Host "== Ferrovox $version" -ForegroundColor Cyan

if ($Repo) {
    $env:FERROVOX_UPDATE_REPO = $Repo
    Write-Host "Update-Quelle: https://github.com/$Repo/releases"
} else {
    Write-Warning "Kein -Repo angegeben: Der Auto-Updater ist in diesem Build abgeschaltet."
}

$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
$sep = [char]0x1f
$env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$cargoHome=/cargo$sep--remap-path-prefix=$root=/ferrovox"
$targets = @("x86_64-pc-windows-msvc", "i686-pc-windows-msvc")
try {
    foreach ($t in $targets) {
        Write-Host "== Build $t" -ForegroundColor Cyan
        cargo build --release --locked --target $t
        if ($LASTEXITCODE -ne 0) { throw "Build fehlgeschlagen ($t)" }
    }
} finally {
    Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
}
foreach ($t in $targets) {
    $bytes = [IO.File]::ReadAllBytes("target\$t\release\ferrovox.exe")
    $text = [Text.Encoding]::ASCII.GetString($bytes) + [Text.Encoding]::Unicode.GetString($bytes)
    foreach ($bad in @(":\Users\", $root)) {
        if ($text.Contains($bad)) { throw "Lokaler Pfad ($bad) in target\$t\release\ferrovox.exe" }
    }
}
Write-Host "Keine lokalen Pfade in den EXEs."

cargo build --release --locked --manifest-path tools\release\Cargo.toml
if ($LASTEXITCODE -ne 0) { throw "Release-Werkzeug konnte nicht gebaut werden" }
$tool = Join-Path $root "tools\release\target\release\ferrovox-release.exe"
& $tool licenses --manifest Cargo.toml --target x86_64-pc-windows-msvc --out target\THIRD_PARTY_LICENSES.txt
if ($LASTEXITCODE -ne 0) { throw "Lizenztexte fehlgeschlagen" }

$iscc = @(
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
    (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 nicht gefunden (winget install JRSoftware.InnoSetup)" }
$out = Join-Path $root "dist\release"
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $out -File | Remove-Item
& $iscc /Qp "/DAppVersion=$version" installer\ferrovox.iss
if ($LASTEXITCODE -ne 0) { throw "Inno Setup fehlgeschlagen" }
$setup = Join-Path $out "Ferrovox-Setup-$version.exe"

$signArgs = @("sign", "--version", $version, "--setup", $setup, "--out", $out)
foreach ($lang in @("de", "en")) {
    $f = "release\notes-$lang.txt"
    if (Test-Path $f) {
        $notes = (Get-Content $f -Raw -Encoding UTF8).Trim()
        if ($notes) { $signArgs += @("--notes-$lang", $notes) }
    }
}
if (-not $env:FERROVOX_SIGNING_KEY) {
    if (-not (Test-Path $KeyFile)) { throw "Kein Signaturschlüssel: FERROVOX_SIGNING_KEY setzen oder $KeyFile anlegen" }
    $signArgs += @("--key", $KeyFile)
}
& $tool @signArgs
if ($LASTEXITCODE -ne 0) { throw "Signieren fehlgeschlagen" }
$public = [regex]::Match((Get-Content src\update.rs -Raw), 'PUBLIC_KEY_HEX: &str = "([0-9a-f]{64})"').Groups[1].Value
& $tool verify --dir $out --public $public
if ($LASTEXITCODE -ne 0) { throw "Signatur passt nicht zum öffentlichen Schlüssel in src\update.rs" }

$body = "## Ferrovox $version`n"
foreach ($lang in @("de", "en")) {
    $f = "release\notes-$lang.txt"
    if (Test-Path $f) { $body += "`n" + (Get-Content $f -Raw -Encoding UTF8).Trim() + "`n" }
}
[IO.File]::WriteAllText((Join-Path $out "RELEASE_NOTES.md"), $body, (New-Object Text.UTF8Encoding $false))

Write-Host "== Fertig: $out" -ForegroundColor Green
Get-ChildItem $out | Format-Table Name, @{ n = "MB"; e = { [math]::Round($_.Length / 1MB, 2) } } -AutoSize
