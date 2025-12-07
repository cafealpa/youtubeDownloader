import os
import io
import zipfile
import requests
import shutil

def install_ffmpeg():
    print("Downloading FFmpeg...")
    url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
    try:
        response = requests.get(url)
        response.raise_for_status()
        print("Download complete. Extracting...")
        
        with zipfile.ZipFile(io.BytesIO(response.content)) as z:
            for file_info in z.infolist():
                if file_info.filename.endswith('bin/ffmpeg.exe') or file_info.filename.endswith('bin/ffprobe.exe'):
                    # Extract to current directory, flattening the path
                    file_info.filename = os.path.basename(file_info.filename)
                    z.extract(file_info, ".")
                    print(f"Extracted: {file_info.filename}")

        print("FFmpeg installation successful!")

    except Exception as e:
        print(f"Error installing FFmpeg: {e}")

if __name__ == "__main__":
    install_ffmpeg()
