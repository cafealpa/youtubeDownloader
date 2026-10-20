# -*- mode: python ; coding: utf-8 -*-
# 빌드 방법 (프로젝트 루트, .venv 활성화 상태에서):
#   pyinstaller YouTubeDownloader.spec --clean --noconfirm
# 결과물: dist/YouTubeDownloader/YouTubeDownloader.exe

from PyInstaller.utils.hooks import collect_all
from pathlib import PureWindowsPath

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

# Qt는 Windows 기본 ICU API(버전 접미사가 없는 함수)를 사용한다.
# PATH의 Poppler/Conda ICU가 수집되면 함수명이 달라 QtWidgets 로딩이 실패한다.
# Windows 제공 DLL을 사용하도록 동명 ICU DLL은 패키지에서 제외한다.
windows_icu_dlls = {'icu.dll', 'icuuc.dll', 'icuin.dll'}
a.binaries = [
    entry for entry in a.binaries
    if PureWindowsPath(entry[0]).name.lower() not in windows_icu_dlls
]

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
