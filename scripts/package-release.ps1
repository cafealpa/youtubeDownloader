param([string]$Version = '2.0.0')
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw '잘못된 버전 형식입니다.' }
$projectRoot = Split-Path $PSScriptRoot -Parent
Push-Location $projectRoot
try {
    $manifest = Get-Content -LiteralPath Cargo.toml -Raw
    if ($manifest -notmatch ('(?m)^version = "' + [regex]::Escape($Version) + '"')) {
        throw 'Cargo.toml과 패키지 버전이 다릅니다.'
    }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw '빌드 실패' }
    $name = "YouTubeDownloader-v$Version-windows-x64"
    $folder = Join-Path $projectRoot "dist\$name"
    if (Test-Path -LiteralPath $folder) { throw "기존 패키지가 있습니다: $folder" }
    $tools = Join-Path $folder 'tools'
    $licenses = Join-Path $folder 'licenses'
    New-Item -ItemType Directory -Path $tools, $licenses -Force | Out-Null
    Copy-Item -LiteralPath target\release\youtube-downloader.exe -Destination "$folder\YouTubeDownloader.exe"
    foreach ($tool in @('yt-dlp.exe', 'ffmpeg.exe', 'ffprobe.exe', 'deno.exe')) {
        Copy-Item -LiteralPath "tools\$tool" -Destination $tools
    }
    Copy-Item -LiteralPath README.md -Destination $folder
    $engineVersion = (& "$tools\yt-dlp.exe" --version).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'yt-dlp 실행 실패' }
    $denoVersion = ((& "$tools\deno.exe" --version) | Select-Object -First 1).Split(' ')[1]
    if ($LASTEXITCODE -ne 0) { throw 'Deno 실행 실패' }
    $ffmpegVersion = ((& "$tools\ffmpeg.exe" -version) | Select-Object -First 1)
    $urls = [ordered]@{
        'yt-dlp-LICENSE.txt' = "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$engineVersion/LICENSE"
        'yt-dlp-THIRD_PARTY_LICENSES.txt' = "https://raw.githubusercontent.com/yt-dlp/yt-dlp/$engineVersion/THIRD_PARTY_LICENSES.txt"
        'deno-LICENSE.md' = "https://raw.githubusercontent.com/denoland/deno/v$denoVersion/LICENSE.md"
        'ffmpeg-COPYING.GPLv3' = 'https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.0.1/COPYING.GPLv3'
        'ffmpeg-LICENSE.md' = 'https://raw.githubusercontent.com/FFmpeg/FFmpeg/n8.0.1/LICENSE.md'
    }
    foreach ($entry in $urls.GetEnumerator()) {
        Invoke-WebRequest -Uri $entry.Value -OutFile (Join-Path $licenses $entry.Key) -UseBasicParsing
    }
    $metadataText = cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw '의존성 조회 실패' }
    $metadata = $metadataText | ConvertFrom-Json
    $index = @('# Rust dependency notices', '')
    foreach ($package in $metadata.packages) {
        if (-not $package.source) { continue }
        $label = "$($package.name)-$($package.version)"
        $index += "$label | $($package.license) | $($package.repository)"
        $root = Split-Path $package.manifest_path -Parent
        $files = @(Get-ChildItem -LiteralPath $root -File | Where-Object { $_.Name -match '^(LICENSE|COPYING|NOTICE|COPYRIGHT)' })
        if ($package.license_file) {
            $files += Get-Item -LiteralPath (Join-Path $root $package.license_file)
        }
        foreach ($file in ($files | Sort-Object FullName -Unique)) {
            Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $licenses "$label-$($file.Name)")
        }
    }
    $index | Set-Content -LiteralPath "$licenses\RUST-DEPENDENCIES.txt" -Encoding UTF8
    @"
YouTube Downloader $Version / Windows x64
ZIP 전체를 압축 해제한 뒤 YouTubeDownloader.exe를 실행하세요.
tools 폴더를 함께 유지해야 합니다. Rust/Python/Node.js 별도 설치는 필요하지 않습니다.
지원 대상: Windows 10/11 x64. 인터넷 연결 필요.
앱의 코드 서명 인증서는 포함되지 않습니다.

Bundled versions:
yt-dlp $engineVersion
Deno $denoVersion
$ffmpegVersion

Third-party programs run as separate processes; their licenses remain applicable.
Licenses and Rust dependency notices: licenses/
yt-dlp source: https://github.com/yt-dlp/yt-dlp/tree/$engineVersion
Deno source: https://github.com/denoland/deno/tree/v$denoVersion
FFmpeg source: https://github.com/FFmpeg/FFmpeg/tree/n8.0.1
FFmpeg source archive: https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz
FFmpeg binary provider and build information: https://github.com/GyanD/codexffmpeg/releases/tag/8.0.1
FFmpeg linked libraries and upstream projects: https://www.gyan.dev/ffmpeg/builds/#libraries
"@ | Set-Content -LiteralPath "$folder\START-HERE.txt" -Encoding UTF8
    $commit = (git rev-parse HEAD).Trim()
    $hashes = Get-ChildItem -LiteralPath $folder -Recurse -File | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($folder.Length + 1).Replace('\', '/')
            size = $_.Length
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    [ordered]@{ version = $Version; commit = $commit; files = @($hashes) } |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$folder\manifest.json" -Encoding UTF8
    $zip = Join-Path $projectRoot "dist\$name.zip"
    if (Test-Path -LiteralPath $zip) { throw "기존 ZIP이 있습니다: $zip" }
    Compress-Archive -LiteralPath $folder -DestinationPath $zip -CompressionLevel Optimal
    $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $name.zip" | Set-Content -LiteralPath "$zip.sha256" -Encoding ASCII
    Write-Output "패키지: $zip"
    Write-Output "SHA-256: $hash"
} finally {
    Pop-Location
}
