set RUSTFLAGS=-L native=.\mpv
cargo build --release
copy ".\mpv\libmpv-2.dll" ".\target\release"
copy ".\angle\d3dcompiler_47.dll" ".\target\release"
copy ".\angle\libEGL.dll" ".\target\release"
copy ".\angle\libGLESv2.dll" ".\target\release"