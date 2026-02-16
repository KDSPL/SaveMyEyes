#!/bin/bash
# Build SaveMyEyes.app bundle for macOS

set -e

cd "$(dirname "$0")/.."

echo "Building SaveMyEyes for macOS..."
cargo build --release -p savemyeyes-macos

APP_NAME="SaveMyEyes.app"
APP_DIR="target/release/$APP_NAME"
CONTENTS="$APP_DIR/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

echo "Creating app bundle structure..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS"
mkdir -p "$RESOURCES"

echo "Copying binary..."
cp target/release/savemyeyes "$MACOS/savemyeyes"

echo "Copying Info.plist..."
cp macos/Info.plist "$CONTENTS/Info.plist"

echo "Creating app icon..."
if [ -f macos/AppIcon.icns ]; then
    echo "Using existing AppIcon.icns"
    cp macos/AppIcon.icns "$RESOURCES/AppIcon.icns"
else
    echo "Generating AppIcon.icns..."
    (cd macos && bash create-icon.sh) || echo "Warning: Could not generate icon"
    if [ -f macos/AppIcon.icns ]; then
        cp macos/AppIcon.icns "$RESOURCES/AppIcon.icns"
    fi
fi

echo ""
echo "✅ SaveMyEyes.app created at: $APP_DIR"
echo ""
echo "To install:"
echo "  cp -r $APP_DIR /Applications/"
echo ""
echo "To run:"
echo "  open $APP_DIR"
echo ""
