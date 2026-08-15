<#
.SYNOPSIS
    Builds the Windows installer (.msi) for rust-example.

.DESCRIPTION
    Compiles the release binary, converts the LICENSE file into the RTF that
    the installer's licence pane requires, and hands both to the WiX Toolset
    along with packaging/windows/rust-example.wxs.

    Requires the WiX v5 command line tool and its UI extension:

        dotnet tool install --global wix --version 5.*
        wix extension add --global WixToolset.UI.wixext/5.*

.PARAMETER Target
    The Rust target triple to build. Defaults to x86_64-pc-windows-msvc; pass
    aarch64-pc-windows-msvc for an ARM64 installer.

.EXAMPLE
    ./packaging/windows/build-msi.ps1
#>

[CmdletBinding()]
param(
    [string] $Target = 'x86_64-pc-windows-msvc'
)

# Stop on the first error, and treat a failing native command as an error too.
# Without the second setting a failed `cargo build` is followed by a confusing
# WiX complaint about a missing file.
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

# Nested Join-Path calls rather than the multi-argument form, which needs
# PowerShell 6; this way the script also runs under Windows PowerShell 5.1.
$repoRoot = (Resolve-Path (Join-Path (Join-Path $PSScriptRoot '..') '..')).Path
Set-Location $repoRoot

# The version lives in Cargo.toml, and nowhere else. Windows Installer only
# understands numeric versions, so a pre-release suffix such as 0.3.0-rc.1 is
# trimmed to 0.3.0 for the MSI while the file name keeps the full version.
$versionLine = Select-String -Path 'Cargo.toml' -Pattern '^version = "(.+)"' | Select-Object -First 1
$cargoVersion = $versionLine.Matches[0].Groups[1].Value
$msiVersion = ($cargoVersion -split '-')[0]

$arch = switch ($Target.Split('-')[0]) {
    'x86_64'  { 'x64' }
    'aarch64' { 'arm64' }
    'i686'    { 'x86' }
    default   { throw "Unsupported target: $Target" }
}

$dist = Join-Path $repoRoot 'dist'
$build = Join-Path $repoRoot 'target\wix'
$exePath = Join-Path $repoRoot "target\$Target\release\rust-example.exe"
$licensePath = Join-Path $repoRoot 'LICENSE'
$licenseRtfPath = Join-Path $build 'license.rtf'
$configExamplePath = Join-Path $repoRoot 'packaging\config.example.toml'
$wxsPath = Join-Path $PSScriptRoot 'rust-example.wxs'
$msiPath = Join-Path $dist "rust-example-$cargoVersion-$arch.msi"

Write-Host "==> Building rust-example $cargoVersion for $Target"
rustup target add $Target
cargo build --release --locked --target $Target

New-Item -ItemType Directory -Force -Path $dist, $build | Out-Null

# WiX's licence pane reads RTF, not plain text. Generating it here keeps the
# text in the installer identical to the repository's LICENSE file.
#
# In single-quoted PowerShell strings a backslash is an ordinary character, so
# the first replacement turns each backslash into two, and the second escapes
# the braces -- the three characters RTF treats as syntax. A licence containing
# non-ASCII characters would need \u escapes on top of this.
Write-Host '==> Converting LICENSE to RTF'
$body = (Get-Content -Path $licensePath -Raw) -replace '\\', '\\' -replace '([{}])', '\$1'
$body = $body -replace "`r`n", "`n" -replace "`n", "\par`n"
"{\rtf1\ansi\deff0{\fonttbl{\f0\fnil\fcharset0 Segoe UI;}}\fs18`n$body`n}" |
    Set-Content -Path $licenseRtfPath -Encoding ascii

Write-Host '==> Building the installer'
wix build `
    -arch $arch `
    -ext WixToolset.UI.wixext `
    -d "Version=$msiVersion" `
    -d "ExePath=$exePath" `
    -d "LicensePath=$licensePath" `
    -d "LicenseRtfPath=$licenseRtfPath" `
    -d "ConfigExamplePath=$configExamplePath" `
    -out $msiPath `
    $wxsPath

Write-Host "==> Built $msiPath"
Get-Item $msiPath | Format-List Name, Length, LastWriteTime
