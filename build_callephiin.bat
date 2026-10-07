set RUSTFLAGS=-L native=.\mpv
cargo build --release
copy ".\mpv\libmpv-2.dll" ".\target\release"