param(
    [ValidatePattern('^\d{4}\.\d{2}\.\d{2}$')]
    [string]$Version = '2026.08.19'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$toolsPath = Join-Path $projectRoot 'tools'
New-Item -ItemType Directory -Path $toolsPath -Force | Out-Null
$releaseBase = "https://github.com/yt-dlp/yt-dlp/releases/download/$Version"
$checksums = (Invoke-WebRequest -Uri "$releaseBase/SHA2-256SUMS" -UseBasicParsing).Content
if ($checksums -is [byte[]]) { $checksums = [Text.Encoding]::UTF8.GetString($checksums) }
$entry = ($checksums -split "`n") | Where-Object { $_ -match '^[a-fA-F0-9]{64}\s+\*?yt-dlp\.exe\s*$' } | Select-Object -First 1
if (-not $entry) { throw '공식 체크섬에서 yt-dlp.exe를 찾지 못했습니다.' }
$expectedHash = ($entry -split '\s+')[0]
$destination = Join-Path $toolsPath 'yt-dlp.exe'
if (-not (Test-Path -LiteralPath $destination) -or (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $expectedHash) {
    $temporary = Join-Path $toolsPath "yt-dlp-$([guid]::NewGuid().ToString('N')).tmp"
    try {
        Invoke-WebRequest -Uri "$releaseBase/yt-dlp.exe" -OutFile $temporary -UseBasicParsing
        if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ne $expectedHash) {
            throw 'yt-dlp 체크섬이 일치하지 않습니다.'
        }
        Move-Item -LiteralPath $temporary -Destination $destination -Force
    } finally {
        if (Test-Path -LiteralPath $temporary) { Remove-Item -LiteralPath $temporary }
    }
}
foreach ($name in @('ffmpeg.exe', 'ffprobe.exe', 'deno.exe')) {
    $sourcePath = Join-Path $projectRoot $name
    if (-not (Test-Path -LiteralPath $sourcePath)) { throw "기존 구성 요소가 없습니다: $sourcePath" }
    Copy-Item -LiteralPath $sourcePath -Destination (Join-Path $toolsPath $name) -Force
}
& $destination --version
if ($LASTEXITCODE -ne 0) { throw 'yt-dlp 실행 확인에 실패했습니다.' }
Write-Output "Sidecar 준비 완료: $toolsPath"
