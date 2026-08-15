param([switch]$Force)

$ErrorActionPreference = "Stop"
$sampleRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

function Ensure-Output([string]$relativePath) {
    $path = Join-Path $sampleRoot $relativePath
    $directory = Split-Path -Parent $path
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
    if ((Test-Path -LiteralPath $path) -and -not $Force) {
        throw "Asset already exists: $path (pass -Force to regenerate)"
    }
    return $path
}

function Write-PcmMelody(
    [string]$relativePath,
    [double[]]$frequencies,
    [double]$noteSeconds,
    [double]$volume
) {
    $path = Ensure-Output $relativePath
    $sampleRate = 22050
    $samplesPerNote = [int]($sampleRate * $noteSeconds)
    $sampleCount = $samplesPerNote * $frequencies.Count
    $dataBytes = $sampleCount * 2
    $stream = [System.IO.File]::Open($path, [System.IO.FileMode]::Create)
    $writer = New-Object System.IO.BinaryWriter($stream)
    try {
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes("RIFF"))
        $writer.Write([int](36 + $dataBytes))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes("WAVEfmt "))
        $writer.Write([int]16)
        $writer.Write([int16]1)
        $writer.Write([int16]1)
        $writer.Write([int]$sampleRate)
        $writer.Write([int]($sampleRate * 2))
        $writer.Write([int16]2)
        $writer.Write([int16]16)
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes("data"))
        $writer.Write([int]$dataBytes)
        foreach ($frequency in $frequencies) {
            for ($i = 0; $i -lt $samplesPerNote; $i++) {
                $time = $i / [double]$sampleRate
                $phase = 2.0 * [Math]::PI * $frequency * $time
                $attack = [Math]::Min(1.0, $i / [double]([Math]::Max(1, $sampleRate * 0.02)))
                $release = [Math]::Min(1.0, ($samplesPerNote - $i) / [double]([Math]::Max(1, $sampleRate * 0.08)))
                $envelope = $attack * $release
                $wave = [Math]::Sin($phase) + 0.22 * [Math]::Sin($phase * 2.0)
                $sample = [int16]([Math]::Max(-32767, [Math]::Min(32767, $wave * $envelope * $volume * 32767)))
                $writer.Write($sample)
            }
        }
    }
    finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

Write-PcmMelody `
    "assets/user/audio/showcase.lantern_theme/lantern_theme.wav" `
    ([double[]](392.00, 440.00, 523.25, 587.33, 523.25, 440.00, 392.00, 329.63,
                349.23, 392.00, 440.00, 523.25, 440.00, 392.00, 349.23, 293.66)) `
    0.28 0.16

Write-PcmMelody `
    "assets/user/audio/showcase.lantern_chime/lantern_chime.wav" `
    ([double[]](1046.50, 1318.51, 1567.98)) `
    0.13 0.23

# A short deterministic voiced cue. It deliberately avoids OS TTS voices so the
# committed sample is reproducible and carries no third-party voice recording.
Write-PcmMelody `
    "assets/user/audio/showcase.lin_greeting/lin_greeting.wav" `
    ([double[]](196.00, 207.65, 220.00, 246.94, 220.00, 207.65, 196.00,
                174.61, 196.00, 220.00, 246.94, 261.63, 246.94, 220.00)) `
    0.16 0.19

Write-Host "Generated deterministic committed showcase audio under $sampleRoot"
