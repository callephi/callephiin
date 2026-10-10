<h1 align="center">
  <br>
    <a href="https://github.com/callephi/callephiin"><img src="https://raw.githubusercontent.com/callephi/callephiin/8837b74e5baa76e8e76d3f84500b563a0ba9a57a/assets/logo.svg" alt="callephiin" width="200"></a>
  <br>
  callephiin
  <br>
</h1>

<h4 align="center">A Jellyfin desktop frontend for Windows built with Rust.</h4>

#### ⚠️ Disclaimer
This project is strictly personal and completely vibecode slop. The only human-made part of this project is the logo, overall design, and this README. Updates will come likely rarely, and bugs may last forever. *However,* I do want this to be an overall good client (at least for the uses I have for it), so if you come across bugs, please, [submit an issue for it.](https://github.com/callephi/callephiin/issues/new)

## Key Features
- MPV backend for video playback with cohesive handling of audio/subtitle tracks
- Discord Presence
- IntroSkipper support
- Multiple profiles & servers
- Spoiler Protection option (disabled by default)
- Custom interface sizing (automatic by default)
- More to be worked on:
  - Nice animations
  - Better handling of missing metadata
  - More detailed Settings page
  - Improved compatibility for external/sidecar files
  - TrickPlay support
  - Support for Apple Silicon
  - Some more fun stuff

## Download
The latest builds for Windows can be found on the [Releases page](https://github.com/callephi/callephiin/releases). 

You need to source your own `libmpv-2.dll` and place it beside `callephiin.exe`. You can download any `mpv-dev-x86_64-*` archive from [SourceForge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/).

## Stack
**UI & Video:** 
- [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/main/crates/eframe) with `egui_extras`
- [resvg](https://github.com/linebender/resvg) for rendering callephiin logo
- [image](https://github.com/image-rs/image)
- [mpv](https://github.com/mpv-player/mpv) via `libmpv2`

**Data & Networking:** 
- [serde](https://github.com/serde-rs/serde)
- [uuid](https://github.com/uuid-rs/uuid)
- [chrono](https://github.com/chronotope/chrono)
- [reqwest](https://github.com/seanmonstar/reqwest) for networking

**Misc:** 
- [discord-rich-presence](https://docs.rs/discord-rich-presence/latest/discord_rich_presence/)
- [anyhow](https://github.com/dtolnay/anyhow)
- [log](https://github.com/rust-lang/log)
- [webbrowser](https://github.com/amodm/webbrowser-rs)
- [winresource](https://github.com/BenjaminRi/winresource)

## Building
Install [Rust's MSVC toolchain](https://rustup.rs), accepting the options as they appear in the installer.

Clone this repo using `git clone https://github.com/callephi/callephiin` to a directory of your choice.

In another folder, extract any `mpv-dev-x86_64-*` archive from [SourceForge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/) (latest preferred) into the `mpv` folder of the repo.

Run `build-callephiin.bat` from the repo root, and the compiled build will be in `/target/release`.
