<h1 align="center">
  <br>
    <a href="https://github.com/callephi/callephiin"><img src="https://raw.githubusercontent.com/callephi/callephiin/8837b74e5baa76e8e76d3f84500b563a0ba9a57a/assets/logo.svg" alt="callephiin" width="200"></a>
  <br>
  callephiin
  <br>
</h1>

<h4 align="center">A multi-platform Jellyfin desktop client built with Rust.</h4>

#### ⚠️ Disclaimer
This project is strictly personal and completely vibecode slop. The only human-made part of this project is the logo, overall design, and this README. Updates will come likely rarely, and bugs may last forever. *However,* I do want this to be an overall good client (at least for the uses I have for it), so if you come across bugs, please, [submit an issue for it.](https://github.com/callephi/callephiin/issues/new)

## Key Features
- MPV backend for video playback with cohesive handling of audio/subtitle tracks
- Discord Presence
- Windows & macOS support
- Sonarr integration (Calendar & Upcoming)
- IntroSkipper support
- Trickplay support
- Multiple profiles & servers
- Spoiler Protection option (disabled by default)
- Custom interface sizing (automatic by default)
- More to be worked on:
  - More detailed Settings page
  - Improved compatibility for external/sidecar files
  - Some more fun stuff

## Download
The latest builds for Windows and macOS (v1.2.0 and later) can be found on the [Releases page](https://github.com/callephi/callephiin/releases).

As of v1.2.0, `libmpv` and `ANGLE` libraries are now included in the release.

## Stack
**UI & Video:** 
- [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/main/crates/eframe) with `egui_extras`
- [resvg](https://github.com/linebender/resvg) for rendering callephiin logo
- [image](https://github.com/image-rs/image)
- [mpv](https://github.com/mpv-player/mpv) via `libmpv2`
- [ffmpeg](https://github.com/FFmpeg/FFmpeg)
- [ANGLE](https://github.com/electron/electron) from Electron (Windows-only)

**Data & Networking:** 
- [serde](https://github.com/serde-rs/serde)
- [uuid](https://github.com/uuid-rs/uuid)
- [chrono](https://github.com/chronotope/chrono)
- [reqwest](https://github.com/seanmonstar/reqwest) for networking
- [zip2](https://github.com/zip-rs/zip2) and [rfd](https://github.com/PolyMeilex/rfd) for config archive import/export

**Misc:** 
- [discord-rich-presence](https://docs.rs/discord-rich-presence/latest/discord_rich_presence/)
- [anyhow](https://github.com/dtolnay/anyhow)
- [log](https://github.com/rust-lang/log)
- [webbrowser](https://github.com/amodm/webbrowser-rs)
- [winresource](https://github.com/BenjaminRi/winresource)

## Building
Install [Rust's MSVC toolchain](https://rustup.rs), plus [Xcode Command Line Tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools) and [Homebrew](https://brew.sh) when building for macOS, accepting the options as they appear in the installers.

### Windows
Clone this repo using `git clone https://github.com/callephi/callephiin` to a directory of your choice.

Download and extract any `mpv-dev-x86_64-*` archive from [SourceForge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/) (latest preferred) into the `mpv` folder of the repo.

In a nearby folder, download and extract any `win32-x64` release from [Electron](https://github.com/electron/electron/releases). Copy the following files into the repo's `angle` folder:
1. `d3dcompiler47.dll`
2. `libEGL.dll`
3. `libGLESv2.dll`

Run `build-callephiin.bat` from the repo root, and the compiled build will be in `/target/release`.

### macOS (Apple Silicon)
Clone this repo using `git clone https://github.com/callephi/callephiin` to a directory of your choice.

Run `build_callephiin_macos.sh` from the directory in Terminal and let it do its thing (it downloads and installs `mpv` and `dylibbundler`, then compiles callephiin). If any dependencies are missing, the script will halt and give you a link on how to proceed.

Final output will be in the `dist` folder as a `.app` and `.dmg` for easy transport.

⚠️ **If the script won't open**:
In the repo folder, run ```chmod +x build_callephiin_macos.sh``` to make the script executable.

## License
[GNU General Public License v3.0 ©](https://github.com/callephi/callephiin/blob/main/LICENSE)
