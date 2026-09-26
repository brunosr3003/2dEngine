#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
[[ $(uname -s) == Darwin ]] || { echo 'Execute este script no Mac.' >&2; exit 1; }
export PATH="$HOME/.cargo/bin:$PATH"
export MMO_API_PADRAO="${MMO_API_PADRAO:-mmo.brunji.com.br:80}"
cargo build --release --bin client
out="$PWD/target/mac"
app="$out/Tempest.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/client "$app/Contents/MacOS/Tempest"
ditto assets "$app/Contents/Resources/assets"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Tempest</string>
<key>CFBundleIdentifier</key><string>com.brunji.tempest</string>
<key>CFBundleName</key><string>Tempest</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.1</string>
<key>CFBundleVersion</key><string>147</string>
<key>NSHighResolutionCapable</key><true/>
<key>CFBundleIconFile</key><string>Tempest</string>
</dict></plist>
PLIST
icons="$out/Tempest.iconset"
mkdir -p "$icons"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/ios/AppIcon-1024.png --out "$icons/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" assets/ios/AppIcon-1024.png --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$icons" -o "$app/Contents/Resources/Tempest.icns"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
plutil -lint "$app/Contents/Info.plist"
ditto -c -k --sequesterRsrc --keepParent "$app" "$out/Tempest-Mac-$(uname -m).zip"
echo "Build pronta: $app"
