#!/usr/bin/env bash
# Packages a macOS Doze.app bundle (with multi-res .icns icon) and Doze-<arch>.dmg + .tar.gz.
# Usage: ./installer/macos/package_macos.sh <binary_path> <arch_label> [out_dir]

set -euo pipefail

BINARY_PATH="${1:?Usage: package_macos.sh <binary_path> <arch_label> [out_dir]}"
ARCH_LABEL="${2:-universal}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUT_DIR="${3:-$REPO_ROOT/dist}"

VERSION="$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
APP_NAME="Doze"
BUNDLE_DIR="$OUT_DIR/${APP_NAME}.app"
CONTENTS_DIR="$BUNDLE_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

mkdir -p "$OUT_DIR"
rm -rf "$BUNDLE_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BINARY_PATH" "$MACOS_DIR/doze"
chmod +x "$MACOS_DIR/doze"

# Build multi-resolution Doze.icns from assets/doze-icon-512.png using built-in sips + iconutil
ICON_SRC="$REPO_ROOT/assets/doze-icon-512.png"
if [[ -f "$ICON_SRC" ]] && command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
    ICONSET_DIR="$(mktemp -d)/Doze.iconset"
    mkdir -p "$ICONSET_DIR"
    sips -z 16 16     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16.png"      >/dev/null
    sips -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16@2x.png"   >/dev/null
    sips -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32.png"      >/dev/null
    sips -z 64 64     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32@2x.png"   >/dev/null
    sips -z 128 128   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128.png"    >/dev/null
    sips -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
    sips -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256.png"    >/dev/null
    sips -z 512 512   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
    cp "$ICON_SRC" "$ICONSET_DIR/icon_512x512.png"
    iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/Doze.icns"
    rm -rf "$(dirname "$ICONSET_DIR")"
fi

cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Doze</string>
    <key>CFBundleExecutable</key>
    <string>doze</string>
    <key>CFBundleIconFile</key>
    <string>Doze</string>
    <key>CFBundleIdentifier</key>
    <string>dev.doze.Doze</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Doze</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

# Ad-hoc sign so macOS Gatekeeper / Apple Silicon allows launching cleanly
if command -v codesign >/dev/null 2>&1; then
    codesign --force --deep --sign - "$BUNDLE_DIR" || true
fi

# Package compressed tar.gz (used by install.sh updater)
TAR_PATH="$OUT_DIR/Doze-macos-${ARCH_LABEL}.tar.gz"
rm -f "$TAR_PATH"
tar -C "$OUT_DIR" -czf "$TAR_PATH" "${APP_NAME}.app"

# Package drag-to-Applications DMG installer
DMG_STAGING="$(mktemp -d)"
cp -R "$BUNDLE_DIR" "$DMG_STAGING/"
ln -s /Applications "$DMG_STAGING/Applications"
DMG_PATH="$OUT_DIR/Doze-macos-${ARCH_LABEL}.dmg"
rm -f "$DMG_PATH"
hdiutil create -volname "Doze ${VERSION}" -srcfolder "$DMG_STAGING" -ov -format UDZO "$DMG_PATH"
rm -rf "$DMG_STAGING"

echo "Packaged macOS artifacts:"
echo "  - $BUNDLE_DIR"
echo "  - $TAR_PATH"
echo "  - $DMG_PATH"
