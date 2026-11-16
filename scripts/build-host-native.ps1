[CmdletBinding()]
param(
    [string]$OutputDirectory = "",
    [ValidateSet("x64", "x86")][string[]]$Architectures = @("x64", "x86")
)

$ErrorActionPreference = "Stop"
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $RepositoryRoot "out/host-native"
}
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $RepositoryRoot $OutputDirectory
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$InstalledTargets = @(& rustup target list --installed)
if ($LASTEXITCODE -ne 0) { throw "Cannot list installed Rust targets." }

Push-Location $RepositoryRoot
try {
    foreach ($Architecture in $Architectures) {
        $Target = if ($Architecture -eq "x64") { "x86_64-pc-windows-msvc" } else { "i686-pc-windows-msvc" }
        if ($InstalledTargets -notcontains $Target) {
            throw "Missing Rust target $Target. Install it once with: rustup target add $Target"
        }
        & cargo --config "target.$Target.rustflags=[`"-C`",`"target-feature=+crt-static`"]" build --locked --release --target $Target -p lom-host-native
        if ($LASTEXITCODE -ne 0) { throw "Native Host kernel build failed: $Target" }
        $Destination = Join-Path $OutputDirectory $Architecture
        [IO.Directory]::CreateDirectory($Destination) | Out-Null
        Copy-Item -LiteralPath (Join-Path $RepositoryRoot "target/$Target/release/lom_host_native.dll") -Destination (Join-Path $Destination "lom_host_native.dll") -ErrorAction Stop
        Write-Host "Built: $Destination/lom_host_native.dll"
    }
}
finally {
    Pop-Location
}
