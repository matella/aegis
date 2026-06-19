# Builds the unsigned Aegis Windows installer (MSI) via cargo-wix / WiX Toolset v3.
# Prereqs (one-time): winget install WiXToolset.WiXToolset ; cargo install cargo-wix
# Run from anywhere:  pwsh -File package.ps1
$ErrorActionPreference = "Stop"
$ws = $PSScriptRoot

# Locate WiX v3 (its bin holds candle.exe/light.exe that cargo-wix drives).
$wix = Get-ChildItem "C:\Program Files (x86)" -Directory -Filter "WiX Toolset v3*" -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
if (-not $wix) { throw "WiX Toolset v3 not found. Install it: winget install WiXToolset.WiXToolset" }
$env:WIX = $wix.FullName + "\"
$env:PATH += ";$env:WIX" + "bin"

# Release binary the MSI ships.
cargo build --release -p aegis-app

# Build the MSI. cargo-wix must run from the package dir so the wxs's package-relative
# paths (wix\License.rtf) resolve; the binary + output point back at the workspace target.
$version = (Select-String -Path "$ws\crates\aegis-app\Cargo.toml" -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
$out = "$ws\target\wix\Aegis-$version-x86_64.msi"
New-Item -ItemType Directory -Force "$ws\target\wix" | Out-Null
Push-Location "$ws\crates\aegis-app"
try {
    cargo wix -p aegis-app --no-build --nocapture --target-bin-dir "$ws\target\release" --output $out
} finally { Pop-Location }

Write-Host "`nMSI built: $out (unsigned)"
