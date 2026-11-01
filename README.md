# Clip Forge

Editor video ala Adobe Premiere untuk Android, landscape-only, Rust + egui.

## Struktur
- `src/lib.rs` – app, dock layout (Project | Program | Effect Controls | Timeline), `android_main`
- `src/timeline.rs` – model Track/Clip, razor, delete, ruler, playhead, drag clip
- `src/main.rs` – entry desktop untuk testing cepat

## Jalankan di desktop (tes UI)
```
cargo run
```

## Build APK
```
rustup target add aarch64-linux-android
cargo install cargo-apk
export ANDROID_HOME=...   # Android SDK
export ANDROID_NDK_ROOT=... # NDK r26+
cargo apk build --release
```
Orientasi dikunci `sensorLandscape` di `Cargo.toml`.

## Shortcut
Space = play/pause, C = razor di playhead, Delete = hapus clip.

## Roadmap
1. Import file (Storage Access Framework via JNI)
2. Decode video: MediaCodec (hardware) -> texture wgpu
3. Audio playback + audio clock sebagai master sync
4. Trim handle kiri/kanan clip, snapping, ripple delete
5. Efek: opacity, crop, speed, transisi, keyframe
6. Export MP4 (MediaCodec encoder + MediaMuxer)
7. Save/load project (serde)
