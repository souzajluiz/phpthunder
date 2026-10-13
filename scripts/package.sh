#!/usr/bin/env bash
# Builds release binaries and packages them for the current platform into
# dist/:
#   macOS:   PhpThunder.app (ad-hoc signed unless CODESIGN_IDENTITY is set)
#            and PhpThunder-<version>-macos-<arch>.dmg
#   Linux:   phpthunder-<version>-linux-<arch>.tar.gz
#   Windows: phpthunder-<version>-windows-<arch>.zip (run from Git Bash)
# Each package holds the IDE and the `phpthunder` language server.
set -euo pipefail

cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
arch=$(uname -m)
dist=dist
mkdir -p "$dist"

cargo build --release -p phpthunder-ide -p phpthunder-lsp

case "$(uname -s)" in
  Darwin)
    app="$dist/PhpThunder.app"
    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/bin"
    cp target/release/phpthunder-ide "$app/Contents/MacOS/PhpThunder"
    # The language server lives apart: on case-insensitive volumes
    # `phpthunder` and `PhpThunder` would be the same file.
    cp target/release/phpthunder "$app/Contents/Resources/bin/phpthunder"
    cp crates/phpthunder-ide/assets/brand/app-icon.icns "$app/Contents/Resources/AppIcon.icns"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>PhpThunder</string>
  <key>CFBundleDisplayName</key><string>PhpThunder</string>
  <key>CFBundleIdentifier</key><string>dev.phpthunder.ide</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleExecutable</key><string>PhpThunder</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    identity="${CODESIGN_IDENTITY:--}"
    codesign --force --deep --sign "$identity" "$app"
    dmg="$dist/PhpThunder-$version-macos-$arch.dmg"
    rm -f "$dmg"
    staging=$(mktemp -d)
    cp -R "$app" "$staging/"
    ln -s /Applications "$staging/Applications"
    hdiutil create -quiet -volname PhpThunder -srcfolder "$staging" -ov -format UDZO "$dmg"
    rm -rf "$staging"
    echo "$app"
    echo "$dmg"
    ;;
  Linux)
    out="$dist/phpthunder-$version-linux-$arch"
    rm -rf "$out"
    mkdir -p "$out"
    cp target/release/phpthunder-ide target/release/phpthunder "$out/"
    cp crates/phpthunder-ide/assets/brand/app-icon-256.png "$out/phpthunder.png"
    cat > "$out/phpthunder.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=PhpThunder
Comment=Native PHP IDE
Exec=phpthunder-ide %F
Icon=phpthunder
Categories=Development;IDE;
DESKTOP
    tar -C "$dist" -czf "$out.tar.gz" "$(basename "$out")"
    echo "$out.tar.gz"
    ;;
  MINGW*|MSYS*|CYGWIN*)
    out="$dist/phpthunder-$version-windows-$arch"
    rm -rf "$out"
    mkdir -p "$out"
    cp target/release/phpthunder-ide.exe target/release/phpthunder.exe "$out/"
    (cd "$dist" && powershell -NoProfile -Command "Compress-Archive -Force -Path '$(basename "$out")' -DestinationPath '$(basename "$out").zip'")
    echo "$out.zip"
    ;;
  *)
    echo "unsupported platform: $(uname -s)" >&2
    exit 1
    ;;
esac
