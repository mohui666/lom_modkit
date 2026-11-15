# Build the native-video-only encoder with an existing MinGW-w64/Git Bash toolchain.
[CmdletBinding()]
param(
    [string]$OutputDirectory = "",
    [string]$ToolchainDirectory = "C:/Strawberry/c/bin",
    [string]$PerlDirectory = "C:/Strawberry/perl/bin",
    [string]$BashPath = "C:/Program Files/Git/bin/bash.exe",
    [int]$Jobs = 4
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0
$RepositoryRoot = Split-Path -Parent $PSScriptRoot
if ($Jobs -lt 1) { throw "Jobs must be a positive integer." }
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $RepositoryRoot "out/video-encoder-9.0.2-minimal"
}
if (-not [IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory = Join-Path $RepositoryRoot $OutputDirectory
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$ToolchainDirectory = [IO.Path]::GetFullPath($ToolchainDirectory)
$PerlDirectory = [IO.Path]::GetFullPath($PerlDirectory)
$BashPath = [IO.Path]::GetFullPath($BashPath)

# Keep archives, extracted sources and their object files in one reusable cache.
$SourcesDirectory = Join-Path $RepositoryRoot "out/video-encoder-9.0.2-minimal/sources"
$X264Commit = "0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee"
$Sources = @(
    [ordered]@{
        name = "ffmpeg-9.0.2"; archive = "ffmpeg-9.0.2.tar.xz"
        url = "https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz"
    },
    [ordered]@{
        name = "x264-$X264Commit"; archive = "x264-$X264Commit.tar.gz"
        url = "https://code.videolan.org/videolan/x264/-/archive/$X264Commit/x264-$X264Commit.tar.gz"
    }
)
foreach ($RequiredTool in @(
    $BashPath,
    (Join-Path $PerlDirectory "perl.exe"),
    (Join-Path $PerlDirectory "pkg-config.bat")
) + @("gcc.exe", "ar.exe", "ranlib.exe", "strip.exe", "gmake.exe", "nasm.exe", "objdump.exe" | ForEach-Object { Join-Path $ToolchainDirectory $_ })) {
    if (-not (Test-Path -LiteralPath $RequiredTool -PathType Leaf)) {
        throw "Required build tool not found: $RequiredTool"
    }
}
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
[IO.Directory]::CreateDirectory($SourcesDirectory) | Out-Null
foreach ($Source in $Sources) {
    if (Test-Path -LiteralPath (Join-Path $SourcesDirectory "$($Source.name)/configure") -PathType Leaf) { continue }
    $Archive = Join-Path $SourcesDirectory $Source.archive
    if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) {
        Invoke-WebRequest -Uri $Source.url -OutFile $Archive
    }
    & (Join-Path $env:SystemRoot "System32/tar.exe") -xf $Archive -C $SourcesDirectory
    if ($LASTEXITCODE -ne 0) { throw "Source extraction failed: $Archive" }
}

# Write generated shell files explicitly as LF/UTF-8, independent of Git's CRLF setting.
$BashScript = @'
#!/usr/bin/env bash
set -euo pipefail
root="$(cygpath -u "$1")"
toolchain="$(cygpath -u "$2")"
perl="$(cygpath -u "$3")"
jobs="$4"
sources="$(cygpath -u "$5")"
root_win="$(cygpath -m "$root")"
export LOM_PERL_DIRECTORY="$perl"
export PATH="$root/tools:$toolchain:$perl:/usr/bin:/bin"
export CC="$toolchain/gcc.exe"
export AR="$toolchain/ar.exe"
export RANLIB="$toolchain/ranlib.exe"
export STRIP="$toolchain/strip.exe"
export PKG_CONFIG="$root/tools/pkg-config"
export PKGCONFIG="$PKG_CONFIG"
export PKG_CONFIG_PATH="$root_win/prefix/lib/pkgconfig"
export PKG_CONFIG_LIBDIR="$PKG_CONFIG_PATH"
export LC_ALL=C
make="$toolchain/gmake.exe"
mkdir -p "$root/logs" "$root/prefix" "$root/tools"
cat > "$root/tools/pkg-config" <<'PKG_CONFIG_EOF'
#!/usr/bin/env bash
exec "$LOM_PERL_DIRECTORY/perl.exe" -x "$LOM_PERL_DIRECTORY/pkg-config.bat" "$@"
PKG_CONFIG_EOF
chmod +x "$root/tools/pkg-config"
{
  "$CC" --version
  "$make" --version
  "$toolchain/nasm.exe" -v
  bash --version
  "$perl/perl.exe" --version
  "$PKG_CONFIG" --version
} > "$root/logs/toolchain.txt" 2>&1

set -x
cd "$sources/x264-0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee"
bash ./configure \
  --prefix="$root_win/prefix" \
  --enable-static --disable-cli --disable-opencl \
  --bit-depth=8 --chroma-format=420 \
  --extra-cflags='-O3 -ffunction-sections -fdata-sections' \
  --extra-ldflags='-static -static-libgcc -Wl,--gc-sections' \
  > "$root/logs/x264-configure.txt" 2>&1
"$make" -j"$jobs" > "$root/logs/x264-build.txt" 2>&1
"$make" install-lib-static install-lib-dev > "$root/logs/x264-install.txt" 2>&1

cd "$sources/ffmpeg-9.0.2"
bash ./configure \
  --prefix="$root_win/prefix" \
  --extra-version=lom-modkit-minimal \
  --cc="$CC" --ar="$AR" --ranlib="$RANLIB" --strip="$STRIP" \
  --pkg-config="$PKG_CONFIG" --pkg-config-flags=--static \
  --target-os=mingw32 --arch=x86_64 \
  --enable-static --disable-shared --disable-autodetect \
  --disable-everything --disable-network --disable-avdevice \
  --disable-ffplay --disable-ffprobe --disable-doc --disable-debug \
  --disable-pthreads --enable-w32threads \
  --enable-gpl --enable-version3 --enable-libx264 \
  --enable-encoder=libx264,aac \
  --enable-decoder=rawvideo,h264,pcm_f32le,pcm_s16le \
  --enable-parser=h264 \
  --enable-demuxer=rawvideo,wav,mov \
  --enable-muxer=mp4 \
  --enable-protocol=file,pipe \
  --enable-filter=amix,scale,format,aresample,aformat,null,anull,trim,atrim \
  --extra-cflags='-O3 -ffunction-sections -fdata-sections' \
  --extra-ldflags='-static -static-libgcc -Wl,--gc-sections' \
  > "$root/logs/ffmpeg-configure.txt" 2>&1
"$make" -j"$jobs" > "$root/logs/ffmpeg-build.txt" 2>&1
cp ffmpeg.exe "$root/ffmpeg.exe"
cp COPYING.GPLv3 "$root/LICENSE"
cp "$sources/x264-0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee/COPYING" "$root/LICENSE.x264"
for section in version buildconf encoders decoders demuxers muxers filters protocols; do
  "$root/ffmpeg.exe" -"$section" > "$root/logs/ffmpeg-$section.txt" 2>&1
done
"$toolchain/objdump.exe" -p "$root/ffmpeg.exe" > "$root/logs/ffmpeg-pe.txt"
wc -c "$root/ffmpeg.exe"
'@
$GeneratedScript = Join-Path $OutputDirectory "build-video-encoder.sh"
$Utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText($GeneratedScript, $BashScript.Replace("`r`n", "`n") + "`n", $Utf8)
$Manifest = [ordered]@{
    ffmpeg = [ordered]@{
        version = "9.0.2"; tag = "n9.0.2"
        tag_object = "ce8f11b9fac4cd7ccfb41ca7c01bc17f5cda8cc3"
        commit = "946fcce07b6dcd0331c8cc609192aeff5e1924f8"
        archive = $Sources[0].url
    }
    x264 = [ordered]@{ commit = $X264Commit; archive = $Sources[1].url }
    artifact_license = "GPL-3.0-or-later"; x264_license = "GPL-2.0-or-later"
    source_directory = $SourcesDirectory
    profile = "native RGBA+PCM to libx264/AAC MP4 only; legacy detector uses external FFmpeg"
    build = [ordered]@{
        entry = "scripts/build-video-encoder.ps1"; shell = $BashPath
        toolchain = $ToolchainDirectory; perl = $PerlDirectory; jobs = $Jobs
    }
}
[IO.File]::WriteAllText((Join-Path $OutputDirectory "source-manifest.json"), (ConvertTo-Json -InputObject $Manifest -Depth 5) + "`n", $Utf8)
Write-Host "Building the native video encoder; logs: $OutputDirectory/logs"
& $BashPath --noprofile --norc $GeneratedScript.Replace('\', '/') $OutputDirectory $ToolchainDirectory $PerlDirectory $Jobs $SourcesDirectory
if ($LASTEXITCODE -ne 0) {
    throw "Video encoder build failed (exit $LASTEXITCODE); see $OutputDirectory/logs."
}
Write-Host "Built: $OutputDirectory/ffmpeg.exe"
