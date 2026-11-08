[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$BundleDirectory = "",
    [string]$OutputDirectory = "",
    [string]$RuntimeDirectory = "",
    [string]$AudioDecoderDirectory = "",
    [switch]$NoArchive,
    [switch]$Archive,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$ScriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepositoryRoot = Split-Path -Parent $ScriptDirectory
$NativeTarget = "x86_64-pc-windows-msvc"
$NativeOutput = Join-Path $RepositoryRoot "target\$NativeTarget\release"

function Read-VersionMatch {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label version source is missing: $Path"
    }
    $Match = [regex]::Match([IO.File]::ReadAllText($Path), $Pattern)
    if (-not $Match.Success) {
        throw "Cannot read $Label version from $Path"
    }
    return $Match.Groups[1].Value
}

function Replace-FileAtomically {
    param(
        [Parameter(Mandatory = $true)][string]$Temporary,
        [Parameter(Mandatory = $true)][string]$Destination
    )
    if ([IO.File]::Exists($Destination)) {
        $Backup = "$Destination.replace-backup-$([Guid]::NewGuid().ToString('N'))"
        try {
            # Defender/Explorer may briefly open a freshly produced archive between
            # verification and replacement. Keep the operation atomic, but tolerate
            # that transient sharing violation instead of asking maintainers to rerun.
            $LastError = $null
            for ($Attempt = 1; $Attempt -le 6; $Attempt++) {
                try {
                    [IO.File]::Replace($Temporary, $Destination, $Backup, $true)
                    $LastError = $null
                    break
                }
                catch {
                    $LastError = $_
                    if ($Attempt -lt 6) {
                        Start-Sleep -Milliseconds 350
                    }
                }
            }
            if ($null -ne $LastError) {
                throw $LastError
            }
        }
        finally {
            if ([IO.File]::Exists($Backup)) {
                [IO.File]::Delete($Backup)
            }
        }
    }
    else {
        [IO.File]::Move($Temporary, $Destination)
    }
}

# Native Rust tools. This script is run on Windows by the maintainer; macOS CI
# does not execute Windows or game validation.
$CargoPath = Join-Path $RepositoryRoot "Cargo.toml"
$EditorVersion = Read-VersionMatch $CargoPath '(?m)^version\s*=\s*"([^"]+)"' "Rust workspace"
if ([string]::IsNullOrWhiteSpace($Version)) { $Version = $EditorVersion }
if ($Version -ne $EditorVersion) { throw "Release version must match Cargo.toml ($EditorVersion)" }
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') { throw "Invalid SemVer: $Version" }
if ([string]::IsNullOrWhiteSpace($RuntimeDirectory)) {
    $RuntimeDirectory = Join-Path $RepositoryRoot "runtime\MortalModHost\bin\Release\net48"
}
foreach ($Name in @("MortalModHost.dll", "NVorbis.dll")) {
    if (-not (Test-Path -LiteralPath (Join-Path $RuntimeDirectory $Name) -PathType Leaf)) {
        throw "Missing C# runtime dependency: $Name. Build Host first, or pass -RuntimeDirectory."
    }
}
Push-Location $RepositoryRoot
try {
    & cargo --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]' build --locked --release --target $NativeTarget -p lomc -p lom-assets
    if ($LASTEXITCODE -ne 0) { throw "Rust build failed: $LASTEXITCODE" }
} finally { Pop-Location }
if ([string]::IsNullOrWhiteSpace($BundleDirectory)) {
    $BundleDirectory = Join-Path $RepositoryRoot "out\windows\lom_modkit"
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory = Join-Path $RepositoryRoot "out\windows" }
# Check every existing ancestor before writing the native bundle (also -NoArchive).
$CheckPath = [IO.Path]::GetFullPath($BundleDirectory)
while (-not [string]::IsNullOrWhiteSpace($CheckPath)) {
    if (Test-Path -LiteralPath $CheckPath) {
        $CheckItem = Get-Item -LiteralPath $CheckPath -Force
        if (($CheckItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Bundle path contains a symlink/junction: $CheckPath" }
    }
    $CheckPath = Split-Path -Parent $CheckPath
}
if (Test-Path -LiteralPath $BundleDirectory) {
    foreach ($Item in Get-ChildItem -LiteralPath $BundleDirectory -Recurse -Force) {
        if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Bundle contains a symlink/junction: $($Item.FullName)" }
    }
}
[IO.Directory]::CreateDirectory($BundleDirectory) | Out-Null
# Components are build inputs, never required beside the distributed editor.
$ComponentDirectory = Join-Path $RepositoryRoot "out\embedded-components"
[IO.Directory]::CreateDirectory($ComponentDirectory) | Out-Null
foreach ($Folder in @("runtime", "asset-extractor", "audio-decoder", "licenses")) {
    [IO.Directory]::CreateDirectory((Join-Path $ComponentDirectory $Folder)) | Out-Null
}
Copy-Item -LiteralPath (Join-Path $NativeOutput "lomc.exe") -Destination (Join-Path $ComponentDirectory "lomc.exe")
Copy-Item -LiteralPath (Join-Path $NativeOutput "lom-asset-extractor.exe") -Destination (Join-Path $ComponentDirectory "asset-extractor\lom-asset-extractor.exe")
& (Join-Path $ComponentDirectory "asset-extractor\lom-asset-extractor.exe") --self-check
if ($LASTEXITCODE -ne 0) { throw "Native preview extractor self-check failed" }
foreach ($Name in @("MortalModHost.dll", "NVorbis.dll")) {
    Copy-Item -LiteralPath (Join-Path $RuntimeDirectory $Name) -Destination (Join-Path $ComponentDirectory "runtime\$Name")
}
# Loaders use the same exact archives authenticated by game_tools.rs.
$LoaderCache = Join-Path $env:LOCALAPPDATA "lom_modkit\cache\loaders"
[IO.Directory]::CreateDirectory($LoaderCache) | Out-Null
foreach ($Architecture in @("x86", "x64")) {
    $Name = "BepInEx-Unity.Mono-win-$Architecture-6.0.0-be.692+851521c.zip"
    $Cached = Join-Path $LoaderCache $Name
    if (-not (Test-Path -LiteralPath $Cached -PathType Leaf)) {
        Invoke-WebRequest -Uri ("https://builds.bepinex.dev/projects/bepinex_be/692/" + $Name.Replace("+", "%2B")) -OutFile $Cached
    }
    $Expected = if ($Architecture -eq "x86") { "97720c5f5c70abfb2ae19dba6000529049ae67f053303b3ce2b49e6ad6c0eca6" } else { "9a3472f5eefb35a84ae8c6dea16814b728aff807c67c14fbfd448e20112951a6" }
    if ((Get-FileHash -LiteralPath $Cached -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected) { throw "Pinned loader checksum mismatch: $Name" }
    Copy-Item -LiteralPath $Cached -Destination (Join-Path $ComponentDirectory "runtime\$Name")
}
Push-Location $RepositoryRoot
try { $Metadata = (& cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc | ConvertFrom-Json) } finally { Pop-Location }
$UnityPackage = $Metadata.packages | Where-Object { $_.name -eq "unity-rs-core" } | Select-Object -First 1
$UnityDirectory = Split-Path -Parent $UnityPackage.manifest_path
foreach ($Name in @("LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt", "FSB_VORBIS_NOTICE.md")) {
    Copy-Item -LiteralPath (Join-Path $UnityDirectory $Name) -Destination (Join-Path $ComponentDirectory "licenses\unity-rs-$Name")
}
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "LICENSE") -Destination (Join-Path $ComponentDirectory "licenses\LICENSE")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "runtime\MortalModHost\THIRD_PARTY_NOTICES.md") -Destination (Join-Path $ComponentDirectory "licenses\Host-NOTICES.md")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "editor\assets\doorstop\LICENSE.UnityDoorstop") -Destination (Join-Path $ComponentDirectory "licenses\LICENSE.UnityDoorstop")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "docs\chs\windows_quick_start.md") -Destination (Join-Path $BundleDirectory "README-Windows.md")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "LICENSE") -Destination (Join-Path $BundleDirectory "LICENSE")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "runtime\MortalModHost\THIRD_PARTY_NOTICES.md") -Destination (Join-Path $BundleDirectory "THIRD_PARTY_NOTICES.md")
# The native audio preview uses the unmodified upstream decoder and its notices.
if ([string]::IsNullOrWhiteSpace($AudioDecoderDirectory)) {
    $AudioDecoderDirectory = Join-Path $RepositoryRoot "out\audio-decoder-r2168\bin"
    if (-not (Test-Path -LiteralPath (Join-Path $AudioDecoderDirectory "vgmstream-cli.exe") -PathType Leaf)) {
        $DecoderCache = Split-Path -Parent $AudioDecoderDirectory
        [IO.Directory]::CreateDirectory($DecoderCache) | Out-Null
        $DecoderArchive = Join-Path $DecoderCache "vgmstream-win64.zip"
        if (-not (Test-Path -LiteralPath $DecoderArchive -PathType Leaf)) {
            Invoke-WebRequest -Uri "https://github.com/vgmstream/vgmstream/releases/download/r2168/vgmstream-win64.zip" -OutFile $DecoderArchive
        }
        Expand-Archive -LiteralPath $DecoderArchive -DestinationPath $AudioDecoderDirectory
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $AudioDecoderDirectory "vgmstream-cli.exe") -PathType Leaf)) {
    throw "Missing audio decoder: $AudioDecoderDirectory"
}
Copy-Item -Path (Join-Path $AudioDecoderDirectory "*") -Destination (Join-Path $ComponentDirectory "audio-decoder") -Recurse -Force
$ComponentPrefix = [IO.Path]::GetFullPath($ComponentDirectory).TrimEnd('\') + '\'
$ComponentManifest = @(Get-ChildItem -LiteralPath $ComponentDirectory -File -Recurse | Where-Object { $_.Name -ne "components.json" } | ForEach-Object {
    [ordered]@{ path = $_.FullName.Substring($ComponentPrefix.Length).Replace("\", "/"); size = $_.Length }
})
[IO.File]::WriteAllText((Join-Path $ComponentDirectory "components.json"), (ConvertTo-Json -InputObject $ComponentManifest -Depth 4), (New-Object Text.UTF8Encoding($false)))
Add-Type -AssemblyName System.IO.Compression.FileSystem
$ComponentsArchive = Join-Path $RepositoryRoot "out\embedded-components.zip"
if (Test-Path -LiteralPath $ComponentsArchive) { Remove-Item -LiteralPath $ComponentsArchive -ErrorAction Stop }
[IO.Compression.ZipFile]::CreateFromDirectory($ComponentDirectory, $ComponentsArchive, [IO.Compression.CompressionLevel]::Optimal, $false)
$PreviousComponents = $env:LOM_EDITOR_COMPONENTS
try {
    $env:LOM_EDITOR_COMPONENTS = $ComponentsArchive
    Push-Location $RepositoryRoot
    try { & cargo --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]' build --locked --release --target $NativeTarget -p lom-editor; if ($LASTEXITCODE -ne 0) { throw "Embedded editor build failed" } } finally { Pop-Location }
} finally { $env:LOM_EDITOR_COMPONENTS = $PreviousComponents }
Copy-Item -LiteralPath (Join-Path $NativeOutput "lom-editor.exe") -Destination (Join-Path $BundleDirectory "lom-editor.exe")
# Remove only obsolete product components from an existing default build output.
$OutPrefix = [IO.Path]::GetFullPath((Join-Path $RepositoryRoot "out")).TrimEnd('\') + '\'
foreach ($Relative in @("asset-extractor", "audio-decoder", "runtime", "assets", "lomc.exe")) {
    $LegacyPath = [IO.Path]::GetFullPath((Join-Path $BundleDirectory $Relative))
    if (Test-Path -LiteralPath $LegacyPath) {
        if (-not $LegacyPath.StartsWith($OutPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Choose an empty bundle directory outside out/: $LegacyPath" }
        Remove-Item -LiteralPath $LegacyPath -Recurse -ErrorAction Stop
    }
}
$BundleDirectory = (Resolve-Path -LiteralPath $BundleDirectory).Path
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$BundleItem = Get-Item -LiteralPath $BundleDirectory -Force
if (($BundleItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Native bundle itself cannot be a symlink/junction: $BundleDirectory"
}
$BundlePrefix = $BundleDirectory.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if ($OutputDirectory.Equals($BundleDirectory, [StringComparison]::OrdinalIgnoreCase) -or
    $OutputDirectory.StartsWith($BundlePrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Output directory must not be inside the frozen bundle: $OutputDirectory"
}

$RequiredFiles = @("README-Windows.md", "LICENSE", "THIRD_PARTY_NOTICES.md", "lom-editor.exe")
foreach ($Relative in $RequiredFiles) {
    $Required = Join-Path $BundleDirectory $Relative
    if (-not (Test-Path -LiteralPath $Required -PathType Leaf)) {
        throw "Native bundle is incomplete; missing $Relative"
    }
}

$ForbiddenDirectoryNames = @(".git", ".pytest_cache", "__pycache__", "bin", "build", "dist", "mods", "obj", "samples", "tests")
$ForbiddenExtensions = @(".cfg", ".log", ".lomcontent", ".lommod", ".pdb", ".pyc")
foreach ($Item in Get-ChildItem -LiteralPath $BundleDirectory -Recurse -Force) {
    if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Native bundle contains a symlink/junction and will not be packaged: $($Item.FullName)"
    }
    if ($Item.PSIsContainer -and $ForbiddenDirectoryNames -contains $Item.Name.ToLowerInvariant()) {
        throw "Native bundle contains forbidden build/user directory: $($Item.FullName)"
    }
    if (-not $Item.PSIsContainer -and $ForbiddenExtensions -contains $Item.Extension.ToLowerInvariant()) {
        throw "Native bundle contains forbidden build/user file: $($Item.FullName)"
    }
}

$StandalonePath = Join-Path $OutputDirectory "lom-editor.exe"
Copy-Item -LiteralPath (Join-Path $BundleDirectory "lom-editor.exe") -Destination $StandalonePath -ErrorAction Stop
Write-Host "OK  $StandalonePath"
if (-not $Archive -or $NoArchive) { return }

$ArchiveName = "lom_modkit-v${Version}_windows_x64.zip"
$ArchivePath = Join-Path $OutputDirectory $ArchiveName
$ChecksumPath = "$ArchivePath.sha256"
if (-not $Force -and ((Test-Path -LiteralPath $ArchivePath) -or (Test-Path -LiteralPath $ChecksumPath))) {
    throw "Release output already exists. Refusing to overwrite without -Force: $ArchivePath"
}

$Nonce = [Guid]::NewGuid().ToString("N")
$TemporaryArchive = Join-Path $OutputDirectory ".$ArchiveName.$Nonce.tmp.zip"
$TemporaryChecksum = Join-Path $OutputDirectory ".$ArchiveName.$Nonce.tmp.sha256"
try {
    Compress-Archive -LiteralPath $BundleDirectory -DestinationPath $TemporaryArchive -CompressionLevel Optimal

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Zip = [IO.Compression.ZipFile]::OpenRead($TemporaryArchive)
    try {
        $Names = @($Zip.Entries | ForEach-Object { $_.FullName.Replace("\", "/") })
        foreach ($Relative in $RequiredFiles) {
            $Expected = "lom_modkit/" + $Relative.Replace("\", "/")
            if ($Names -notcontains $Expected) {
                throw "Temporary release archive failed verification; missing $Expected"
            }
        }
        foreach ($Name in $Names) {
            if (-not $Name.StartsWith("lom_modkit/", [StringComparison]::Ordinal)) {
                throw "Temporary release archive contains an unexpected top-level path: $Name"
            }
        }
    }
    finally {
        $Zip.Dispose()
    }

    $Digest = (Get-FileHash -LiteralPath $TemporaryArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    $ChecksumText = "$Digest  $ArchiveName`n"
    [IO.File]::WriteAllText($TemporaryChecksum, $ChecksumText, (New-Object Text.UTF8Encoding($false)))

    Replace-FileAtomically $TemporaryArchive $ArchivePath
    Replace-FileAtomically $TemporaryChecksum $ChecksumPath
    Write-Host "OK  $ArchivePath"
    Write-Host "OK  $ChecksumPath"
    Write-Host "SHA256 $Digest"
}
finally {
    if (Test-Path -LiteralPath $TemporaryArchive) {
        Remove-Item -LiteralPath $TemporaryArchive -Force
    }
    if (Test-Path -LiteralPath $TemporaryChecksum) {
        Remove-Item -LiteralPath $TemporaryChecksum -Force
    }
}
