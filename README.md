<h1 align="center">
  <br>
    <a href="https://github.com/callephi/callephiin"><img src="https://raw.githubusercontent.com/callephi/callephiin/assets/logo.svg" alt="callephiin" width="200"></a>
  <br>
  callephiin
  <br>
</h1>

<h4 align="center">A Jellyfin desktop frontend for Windows built with Rust.</h4>

#### ⚠️ Disclaimer
This project is strictly personal and completely vibecode slop. The only human-made part of this project is the overall design as well as the logo. Updates will come likely rarely, and bugs may last forever. *However,* I do want this to be an overall good client (at least for the uses I have for it), so if you come across bugs, please, [submit an issue for it.](https://github.com/callephi/callephiin/issues/new)

## Key Features
- MPV backend for video playback with cohesive handling of audio/subtitle tracks
- Discord Presence
- IntroSkipper support
- TrickPlay support
- More to be worked on

## Download
The latest builds for Windows can be found on the [Releases page](https://github.com/callephi/callephiin/releases). 

You need to source your own `libmpv-2.dll` and place it beside `callephiin.exe`. You can download any `mpv-dev-x86_64-*` archive from [SourceForge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/).

## Building
Install [Rust's MSVC toolchain](https://rustup.rs), accepting the options as they appear in the installer.

Clone this repo using `git clone https://github.com/callephi/callephiin` to a directory of your choice.

In another folder, extract any `mpv-dev-x86_64-*` archive from [SourceForge](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/) (latest preferred) to a folder close to your repo clone.

In the repo's `mpv` folder, edit `build-callephiin.bat` and change `cd C:\YOUR_DIRECTORY_HERE` to the folder where you cloned the repo.

The compiled build will be in `/target/release`.
