# -*- mode: python ; coding: utf-8 -*-
# 빌드 방법 (프로젝트 루트, .venv 활성화 상태에서):
#   pyinstaller YouTubeDownloader.spec --noconfirm
# 결과물: dist/YouTubeDownloader/YouTubeDownloader.exe

from PyInstaller.utils.hooks import collect_all

# yt-dlp와 yt-dlp-ejs(내장 JS 스크립트 데이터 포함)를 통째로 수집
ytdlp_datas, ytdlp_binaries, ytdlp_hiddenimports = collect_all('yt_dlp')
ejs_datas, ejs_binaries, ejs_hiddenimports = collect_all('yt_dlp_ejs')

a = Analysis(
    ['main.py'],
    pathex=[],
    binaries=ytdlp_binaries + ejs_binaries,
    datas=[
        ('ffmpeg.exe', '.'),
        ('ffprobe.exe', '.'),
        ('deno.exe', '.'),
    ] + ytdlp_datas + ejs_datas,
    hiddenimports=ytdlp_hiddenimports + ejs_hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='YouTubeDownloader',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    console=False,
)
coll = COLLECT(
    exe,
    a.binaries,
    a.datas,
    strip=False,
    upx=False,
    name='YouTubeDownloader',
)
