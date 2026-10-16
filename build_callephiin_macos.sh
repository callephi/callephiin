#!/usr/bin/env bash
# Builds callephiin for Apple Silicon and packages it as dist/callephiin.app and dist/callephiin-macos-arm64.dmg.
# Run on a Mac:  ./build_callephiin_macos.sh      (first run: chmod +x build_callephiin_macos.sh)
# Needs Xcode command line tools, Homebrew and Rust (https://rustup.rs); mpv and dylibbundler are installed here.
set -euo pipefail
cd "$(dirname "$0")"

TARGET=aarch64-apple-darwin
APP=dist/callephiin.app

command -v brew >/dev/null || { echo "Homebrew is required: https://brew.sh"; exit 1; }
command -v cargo >/dev/null || { echo "Rust is required: https://rustup.rs"; exit 1; }
brew list mpv >/dev/null 2>&1 || brew install mpv
command -v dylibbundler >/dev/null || brew install dylibbundler
command -v rustup >/dev/null && rustup target add "$TARGET"

MPV_LIB="$(brew --prefix mpv)/lib"
export RUSTFLAGS="${RUSTFLAGS:-} -L native=$MPV_LIB"
cargo build --release --target "$TARGET"

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
rm -rf dist && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "target/$TARGET/release/callephiin" "$APP/Contents/MacOS/callephiin"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"

# app icon from assets/icon.ico (skipped if sips cannot read it)
if [ -f assets/icon.ico ]; then
  ICONSET="$(mktemp -d)/AppIcon.iconset"; mkdir -p "$ICONSET"
  if sips -s format png assets/icon.ico --out "$ICONSET/base.png" >/dev/null 2>&1; then
    for s in 16 32 128 256 512; do
      sips -z $s $s "$ICONSET/base.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
      sips -z $((s*2)) $((s*2)) "$ICONSET/base.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
    done
    rm "$ICONSET/base.png"
    iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns" || echo "icon skipped"
  else
    echo "icon skipped (sips could not read assets/icon.ico)"
  fi
fi

# copy libmpv and everything it needs into the app, and point the binary at them
dylibbundler -b -cd -of -x "$APP/Contents/MacOS/callephiin" -d "$APP/Contents/Frameworks/" -p @executable_path/../Frameworks/ -s "$MPV_LIB"

# dylibbundler can add the same rpath more than once, and dyld refuses to launch a binary with duplicates
# (the bundled dylibs get them too, and dyld reports that against the dylib it was loading)
for f in "$APP/Contents/MacOS/callephiin" "$APP/Contents/Frameworks/"*.dylib; do
  otool -l "$f" | awk '/cmd LC_RPATH/{r=1} r&&/ path /{print $2; r=0}' | sort -u | while read -r rp; do
    while [ "$(otool -l "$f" | grep -c "path $rp ")" -gt 1 ]; do
      install_name_tool -delete_rpath "$rp" "$f"
    done
  done
done
if [ -n "$(otool -l "$APP/Contents/MacOS/callephiin" | grep -c 'path @executable_path/../Frameworks/ ' | grep -x 0)" ]; then
  install_name_tool -add_rpath @executable_path/../Frameworks/ "$APP/Contents/MacOS/callephiin"
fi

# Apple Silicon only runs signed code; this is an ad-hoc signature (no Apple Developer account involved)
codesign --force --deep --sign - "$APP"

hdiutil create -volname callephiin -srcfolder "$APP" -ov -format UDZO "dist/callephiin-macos-arm64.dmg" >/dev/null
echo
echo "Done: $APP and dist/callephiin-macos-arm64.dmg"
echo "If macOS says the app is damaged or from an unidentified developer (a downloaded copy), run:"
echo "  xattr -dr com.apple.quarantine /Applications/callephiin.app"
