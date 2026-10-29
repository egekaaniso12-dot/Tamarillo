# Sidecar binaries

Place the tested, licensed Windows x64 builds here using Tauri target suffixes:

- `ffmpeg-x86_64-pc-windows-msvc.exe`
- `ffprobe-x86_64-pc-windows-msvc.exe`
- `yt-dlp-x86_64-pc-windows-msvc.exe`

Tauri resolves `externalBin` entries by target triple. Keep each upstream version and SHA-256 in `tools-manifest.json`; include the applicable license texts in `licenses/` before publishing releases. Do not commit unverified executables.
