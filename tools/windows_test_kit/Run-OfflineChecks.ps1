[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$BundleDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$KitRoot = $PSScriptRoot
$BundleDirectory = (Resolve-Path -LiteralPath $BundleDirectory).Path
$Cli = Join-Path $BundleDirectory 'lomc.exe'
$Editor = Join-Path $BundleDirectory 'lom-editor.exe'
$Extractor = Join-Path $BundleDirectory 'asset-extractor\lom-asset-extractor.exe'
foreach ($Path in @($Cli, $Editor, $Extractor)) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing bundled executable: $Path" }
}
$RunDirectory = Join-Path $KitRoot ('results\' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,6))
[IO.Directory]::CreateDirectory($RunDirectory) | Out-Null
$Results = New-Object 'System.Collections.Generic.List[object]'
$Utf8 = New-Object System.Text.UTF8Encoding($false)
function Invoke-Case {
    param([string]$Name, [string]$Exe, [string[]]$Arguments, [bool]$Reject = $false, [string]$Needle = '')
    $PreviousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $Output = (& $Exe @Arguments 2>&1 | Out-String)
        $Code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $PreviousPreference }
    [IO.File]::WriteAllText((Join-Path $RunDirectory ($Name + '.log')), $Output, $Utf8)
    $Passed = if ($Reject) { $Code -ne 0 } else { $Code -eq 0 }
    if ($Needle -and -not $Output.Contains($Needle)) { $Passed = $false }
    $Results.Add([pscustomobject]@{ check=$Name; passed=$Passed; exit_code=$Code; expected_rejection=$Reject })
    Write-Host (('{0} {1}' -f $(if ($Passed) {'PASS'} else {'FAIL'}), $Name))
}
$env:PYTHONIOENCODING = 'utf-8'
[Console]::OutputEncoding = $Utf8
Invoke-Case -Name 'compiler_version' -Exe $Cli -Arguments @('--version')
Invoke-Case -Name 'extractor_self_check' -Exe $Extractor -Arguments @('--self-check') -Needle '"sprite_decoder_ready": true'
$Projects = Get-Content -LiteralPath (Join-Path $KitRoot 'test-projects.json') -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($Project in $Projects) {
    $Directory = Join-Path $KitRoot $Project.path
    foreach ($Story in Get-ChildItem -LiteralPath (Join-Path $Directory 'story') -Filter '*.json' -File) {
        Invoke-Case -Name ($Project.id + '_' + $Story.BaseName) -Exe $Cli -Arguments @('check',$Story.FullName,'--json')
    }
    $Package = Join-Path $RunDirectory ($Project.id + '.lommod')
    Invoke-Case -Name ($Project.id + '_pack') -Exe $Cli -Arguments @('pack',$Directory,'-o',$Package,'--json')
    if (Test-Path -LiteralPath $Package) {
        Invoke-Case -Name ($Project.id + '_inspect') -Exe $Cli -Arguments @('inspect',$Package,'--json')
    }
    Invoke-Case -Name ($Project.id + '_preview') -Exe $Editor -Arguments @('--smoke-preview',$Directory)
}
Invoke-Case -Name 'editor_offline_tests' -Exe $Cli -Arguments @('test',(Join-Path $KitRoot 'projects\04_editor'),'--json')
$Cases = Get-Content -LiteralPath (Join-Path $KitRoot 'negative-cases.json') -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($Case in $Cases) {
    Invoke-Case -Name $Case.id -Exe $Cli -Arguments @('check',(Join-Path $KitRoot $Case.path),'--json') -Reject $true -Needle $Case.expect_error_contains
}
$Failed = @($Results | Where-Object { -not $_.passed })
$Summary = [ordered]@{ platform=[Environment]::OSVersion.VersionString; bundle=$BundleDirectory; game_started=$false; gui_tested=$false; passed=($Failed.Count -eq 0); results=@($Results.ToArray()) }
[IO.File]::WriteAllText((Join-Path $RunDirectory 'summary.json'), ($Summary | ConvertTo-Json -Depth 12), $Utf8)
Write-Host ('Results: ' + $RunDirectory)
if ($Failed.Count -gt 0) { throw ('Failed checks: ' + $Failed.Count) }
Write-Host 'Offline checks passed. Continue with the manual Windows checklist.'
