# Build a no-install Windows bundle. Run only from GitHub's Windows CI.
# Dependencies are pinned and checked by SHA-256 before extraction.
param(
    [string]$Output = "carta-space-windows-x64-portable.zip"
)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$work = Join-Path $env:RUNNER_TEMP "carta-portable-build"
$stage = Join-Path $work "CartaSpace"
if (Test-Path $work) { Remove-Item -LiteralPath $work -Recurse -Force }
New-Item -Path $stage -ItemType Directory -Force | Out-Null
New-Item -Path (Join-Path $stage "tools") -ItemType Directory -Force | Out-Null

function Download-Verified([string]$uri, [string]$sha, [string]$file) {
    Invoke-WebRequest -Uri $uri -OutFile $file
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash
    if ($actual -ine $sha) {
        throw "SHA-256 verification failed for $uri. Expected $sha; got $actual."
    }
}

$gitArchive = Join-Path $work "mingit.zip"
Download-Verified `
    "https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.2/MinGit-2.56.0.2-64-bit.zip" `
    "da35e72aa21c005a5a0d298cfbae110bc1609a815730ea0dde84b01a1b3cd3be" `
    $gitArchive
Expand-Archive -LiteralPath $gitArchive -DestinationPath (Join-Path $stage "git")
if (-not (Test-Path (Join-Path $stage "git/cmd/git.exe"))) {
    throw "MinGit archive is missing cmd/git.exe"
}

$typstArchive = Join-Path $work "typst.zip"
Download-Verified `
    "https://github.com/typst/typst/releases/download/v0.15.1/typst-x86_64-pc-windows-msvc.zip" `
    "19ce3551153c2fe7ee9fa2f95208310c8f4d3209fedb699e0333faf8913f6736" `
    $typstArchive
$typstExtract = Join-Path $work "typst-extract"
Expand-Archive -LiteralPath $typstArchive -DestinationPath $typstExtract
$typst = @(Get-ChildItem -LiteralPath $typstExtract -Filter "typst.exe" -Recurse -File)
if ($typst.Count -ne 1) { throw "Expected exactly one typst.exe" }
Copy-Item -LiteralPath $typst[0].FullName -Destination (Join-Path $stage "tools/typst.exe")

Copy-Item -LiteralPath (Join-Path $repo "target/release/carta-cli.exe") -Destination $stage
Copy-Item -LiteralPath (Join-Path $repo "crates/carta-gui/target/release/carta-gui.exe") -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot "Start-Carta.cmd") -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot "README-Windows.txt") -Destination $stage
# Carry third-party notices with the executables; sources are pinned to
# exactly the same tags as the verified binary downloads above.
$licenses = Join-Path $stage "licenses"
New-Item -Path $licenses -ItemType Directory -Force | Out-Null
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/git-for-windows/git/v2.56.0.windows.2/COPYING" -OutFile (Join-Path $licenses "Git-COPYING")
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/typst/typst/v0.15.1/LICENSE" -OutFile (Join-Path $licenses "Typst-LICENSE")
Invoke-WebRequest -Uri "https://raw.githubusercontent.com/typst/typst/v0.15.1/NOTICE" -OutFile (Join-Path $licenses "Typst-NOTICE")
Copy-Item -LiteralPath (Join-Path $repo "LICENSES/OFL-iosevka.txt") -Destination (Join-Path $licenses "Iosevka-OFL.txt")
if (Test-Path $Output) { Remove-Item -LiteralPath $Output -Force }
Compress-Archive -Path $stage -DestinationPath $Output -CompressionLevel Optimal
Write-Host "Created $Output"
