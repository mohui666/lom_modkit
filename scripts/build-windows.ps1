[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$BundleDirectory = "",
    [string]$OutputDirectory = "",
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

$AppVersionPath = Join-Path $RepositoryRoot "editor\app_version.py"
$CompilerVersionPath = Join-Path $RepositoryRoot "compiler\lomc\__init__.py"
$PluginPath = Join-Path $RepositoryRoot "runtime\MortalModHost\src\Plugin.cs"
$EditorVersion = Read-VersionMatch $AppVersionPath 'EDITOR_VERSION\s*=\s*"([^"]+)"' "Editor"
$BundledRuntimeVersion = Read-VersionMatch $AppVersionPath 'RUNTIME_VERSION\s*=\s*"([^"]+)"' "bundled Runtime"
$CompilerVersion = Read-VersionMatch $CompilerVersionPath '__version__\s*=\s*"([^"]+)"' "Compiler"
$PluginVersion = Read-VersionMatch $PluginPath 'VERSION\s*=\s*"([^"]+)"' "Runtime plugin"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = $EditorVersion
}
if ($Version -notmatch '^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Release version is not SemVer: $Version"
}
$VersionSources = @{
    "Editor" = $EditorVersion
    "bundled Runtime" = $BundledRuntimeVersion
    "Compiler" = $CompilerVersion
    "Runtime plugin" = $PluginVersion
}
foreach ($Item in $VersionSources.GetEnumerator()) {
    if ($Item.Value -ne $Version) {
        throw "$($Item.Key) version $($Item.Value) does not match release $Version"
    }
}

if ([string]::IsNullOrWhiteSpace($BundleDirectory)) {
    $BundleDirectory = Join-Path $RepositoryRoot "editor\dist\lom_modkit"
}
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = $RepositoryRoot
}
if (-not (Test-Path -LiteralPath $BundleDirectory -PathType Container)) {
    throw "Frozen bundle is missing: $BundleDirectory. Run editor/build_exe.py first."
}
$BundleDirectory = (Resolve-Path -LiteralPath $BundleDirectory).Path
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$BundleItem = Get-Item -LiteralPath $BundleDirectory -Force
if (($BundleItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw "Frozen bundle itself cannot be a symlink/junction: $BundleDirectory"
}
$BundlePrefix = $BundleDirectory.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
if ($OutputDirectory.Equals($BundleDirectory, [StringComparison]::OrdinalIgnoreCase) -or
    $OutputDirectory.StartsWith($BundlePrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Output directory must not be inside the frozen bundle: $OutputDirectory"
}

$RequiredFiles = @(
    "lom_editor.exe",
    "story_api_cli.exe",
    "_internal\runtime\MortalModHost.dll",
    "_internal\runtime\NVorbis.dll",
    "_internal\assets\doorstop\win-x86-doorstop.dll",
    "_internal\data\editor_data.json",
    "_internal\data\preview_map.json"
)
foreach ($Relative in $RequiredFiles) {
    $Required = Join-Path $BundleDirectory $Relative
    if (-not (Test-Path -LiteralPath $Required -PathType Leaf)) {
        throw "Frozen bundle is incomplete; missing $Relative"
    }
}

$ForbiddenDirectoryNames = @(".git", ".pytest_cache", "__pycache__", "bin", "build", "dist", "mods", "obj", "samples", "tests")
$ForbiddenExtensions = @(".cfg", ".log", ".lomcontent", ".lommod", ".pdb", ".pyc")
foreach ($Item in Get-ChildItem -LiteralPath $BundleDirectory -Recurse -Force) {
    if (($Item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Frozen bundle contains a symlink/junction and will not be packaged: $($Item.FullName)"
    }
    if ($Item.PSIsContainer -and $ForbiddenDirectoryNames -contains $Item.Name.ToLowerInvariant()) {
        throw "Frozen bundle contains forbidden build/user directory: $($Item.FullName)"
    }
    if (-not $Item.PSIsContainer -and $ForbiddenExtensions -contains $Item.Extension.ToLowerInvariant()) {
        throw "Frozen bundle contains forbidden build/user file: $($Item.FullName)"
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
