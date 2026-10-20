$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$toolsPath = Join-Path $projectRoot 'tools'
foreach ($name in @('yt-dlp.exe', 'ffmpeg.exe', 'ffprobe.exe', 'deno.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $toolsPath $name))) {
        throw '먼저 scripts/prepare-sidecar.ps1을 실행해 주세요.'
    }
}
Push-Location $projectRoot
try {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust 빌드 실패' }
    $packagePath = Join-Path $projectRoot 'dist\YouTubeDownloader-Iced'
    New-Item -ItemType Directory -Path (Join-Path $packagePath 'tools') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target\release\youtube-downloader.exe') -Destination (Join-Path $packagePath 'YouTubeDownloader.exe') -Force
    foreach ($name in @('yt-dlp.exe', 'ffmpeg.exe', 'ffprobe.exe', 'deno.exe')) {
        Copy-Item -LiteralPath (Join-Path $toolsPath $name) -Destination (Join-Path $packagePath "tools\$name") -Force
    }
    Write-Output "로컬 실행 패키지: $packagePath"
    Write-Output '공개 배포 전에는 포함된 도구의 라이선스 고지를 함께 준비해 주세요.'
} finally {
    Pop-Location
}
