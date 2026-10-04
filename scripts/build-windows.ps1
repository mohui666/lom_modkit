[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$BundleDirectory = "",
    [string]$OutputDirectory = "",
    [string]$RuntimeDirectory = "",
    [switch]$NoArchive,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$ScriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepositoryRoot = Split-Path -Parent $ScriptDirectory

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
    & cargo build --locked --release -p lomc -p lom-editor
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
[IO.Directory]::CreateDirectory((Join-Path $BundleDirectory "runtime")) | Out-Null
[IO.Directory]::CreateDirectory((Join-Path $BundleDirectory "assets\doorstop")) | Out-Null
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "target\release\lom-editor.exe") -Destination (Join-Path $BundleDirectory "lom-editor.exe")
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "target\release\lomc.exe") -Destination (Join-Path $BundleDirectory "lomc.exe")
foreach ($Name in @("MortalModHost.dll", "NVorbis.dll")) {
    Copy-Item -LiteralPath (Join-Path $RuntimeDirectory $Name) -Destination (Join-Path $BundleDirectory "runtime\$Name")
}
Copy-Item -LiteralPath (Join-Path $RepositoryRoot "editor\assets\doorstop\win-x86-doorstop.dll") -Destination (Join-Path $BundleDirectory "assets\doorstop\win-x86-doorstop.dll")
if ($NoArchive) { Write-Host "Built native tools: $BundleDirectory"; return }
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

$RequiredFiles = @(
    "lom-editor.exe",
    "lomc.exe",
    "runtime\MortalModHost.dll",
    "runtime\NVorbis.dll",
    "assets\doorstop\win-x86-doorstop.dll"
)
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
