$ErrorActionPreference = 'Stop'
$cleanupStage = $null
$job = $null
try {
    $jobFile = [IO.Path]::GetFullPath($env:LOM_EDITOR_UPDATE_JOB)
    $job = Get-Content -LiteralPath $jobFile -Raw -Encoding UTF8 | ConvertFrom-Json
    $stage = [IO.Path]::GetFullPath([string]$job.stage)
    $workRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'lom_modkit\cache\work'))
    if ((Split-Path -Parent $jobFile) -ne $stage -or (Split-Path -Parent $stage) -ne $workRoot -or -not (Split-Path -Leaf $stage).StartsWith('lom-modkit-')) { throw 'Invalid update workspace' }
    if (((Get-Item -LiteralPath $stage -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Update workspace cannot be a link' }
    $cleanupStage = $stage
    $root = [IO.Path]::GetFullPath([string]$job.root)
    $rootPrefix = $root.TrimEnd('\') + '\'
    $bundle = Join-Path $stage 'bundle'
    $files = @($job.files)
    if ($files -notcontains 'lom-editor.exe') { throw 'Missing editor update' }
    $targets = @()
    foreach ($relative in $files) {
        $source = [IO.Path]::GetFullPath((Join-Path $bundle $relative))
        $target = [IO.Path]::GetFullPath((Join-Path $root $relative))
        if (-not $source.StartsWith($bundle + '\', [StringComparison]::OrdinalIgnoreCase) -or -not $target.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Update path escapes its directory' }
        $ancestor = $target
        while ($ancestor) {
            if (Test-Path -LiteralPath $ancestor) {
                if (((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Update path contains a link: $ancestor" }
            }
            $ancestor = Split-Path -Parent $ancestor
        }
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing update file: $relative" }
        $targets += [PSCustomObject]@{ Source=$source; Target=$target; Relative=$relative }
    }
    $parent = Get-Process -Id ([int]$job.parent_pid) -ErrorAction SilentlyContinue
    if ($parent -and -not $parent.WaitForExit(120000)) { throw 'The editor did not exit; update cancelled' }
    # Test write access and existing-file locks before replacing any program file.
    foreach ($file in $targets) {
        if (Test-Path -LiteralPath $file.Target) {
            $handle = [IO.File]::Open($file.Target, [IO.FileMode]::Open, [IO.FileAccess]::Write, [IO.FileShare]::None)
            $handle.Dispose()
        }
    }
    foreach ($file in ($targets | Sort-Object { $_.Relative -eq 'lom-editor.exe' })) {
        $parentDirectory = Split-Path -Parent $file.Target
        [IO.Directory]::CreateDirectory($parentDirectory) | Out-Null
        $temporary = Join-Path $parentDirectory ('.lom-update-' + [Guid]::NewGuid().ToString('N'))
        try {
            [IO.File]::Copy($file.Source, $temporary, $false)
            if ([IO.File]::Exists($file.Target)) { [IO.File]::Replace($temporary, $file.Target, [NullString]::Value) }
            else { [IO.File]::Move($temporary, $file.Target) }
        } finally {
            if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary -ErrorAction Stop }
        }
    }
    if ($job.restart) {
        $editor = Join-Path $root 'lom-editor.exe'
        if ($job.project) {
            $argument = '"' + [regex]::Replace([string]$job.project, '(\\+)$', '$1$1') + '"'
            Start-Process -FilePath $editor -ArgumentList $argument -WorkingDirectory $root
        } else { Start-Process -FilePath $editor -WorkingDirectory $root }
    }
} catch {
    if ($job -and -not $job.restart) { [Console]::Error.WriteLine($_.Exception.Message) }
    else {
        Add-Type -AssemblyName System.Windows.Forms
        [Windows.Forms.MessageBox]::Show("编辑器更新未完成：`n$($_.Exception.Message)`n请重新打开编辑器后重试，或从 GitHub 下载完整包。", 'lom_modkit', 'OK', 'Error') | Out-Null
    }
    exit 1
} finally {
    if ($cleanupStage -and (Test-Path -LiteralPath $cleanupStage)) { Remove-Item -LiteralPath $cleanupStage -Recurse -ErrorAction Stop }
}
