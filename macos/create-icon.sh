#!/bin/bash
# Generate SaveMyEyes.icns icon for macOS

set -e

cd "$(dirname "$0")"

ICONSET="AppIcon.iconset"
ICNS="AppIcon.icns"

echo "Creating icon set directory..."
mkdir -p "$ICONSET"

# Create a simple purple eye icon using ImageMagick (if available) or sips
# For now, we'll create a placeholder using the .ico if imagemagick is available

if command -v convert &> /dev/null; then
    echo "Using ImageMagick to create icons..."
    
    # Convert the Windows .ico to png first
    if [ -f "../windows/resources/icon.ico" ]; then
        convert "../windows/resources/icon.ico[0]" -resize 1024x1024 temp_1024.png
    else
        # Create a simple purple circle with an eye shape
        convert -size 1024x1024 xc:none \
            -fill "#7C3AED" \
            -draw "circle 512,512 512,50" \
            -fill white \
            -draw "ellipse 512,512 200,280 0,360" \
            -fill "#7C3AED" \
            -draw "circle 512,512 400,512" \
            temp_1024.png
    fi
    
    # Generate all required sizes
    for size in 16 32 64 128 256 512 1024; do
        convert temp_1024.png -resize ${size}x${size} "$ICONSET/icon_${size}x${size}.png"
        if [ $size -le 512 ]; then
            convert temp_1024.png -resize $((size*2))x$((size*2)) "$ICONSET/icon_${size}x${size}@2x.png"
        fi
    done
    
    rm temp_1024.png
    
elif command -v sips &> /dev/null; then
    echo "ImageMagick not found. Using basic icon generation with sips..."
    
    # Create a simple colored base image
    # Since sips can't create from scratch easily, we'll just copy if .ico exists
    if [ -f "../windows/resources/icon.ico" ]; then
        # sips can convert ico to png
        sips -s format png "../windows/resources/icon.ico" --out temp_base.png 2>/dev/null || {
            echo "Warning: Could not convert .ico. Creating placeholder."
            # Create a simple purple square as fallback
            cat > temp_base.svg << 'EOF'
<svg width="1024" height="1024" xmlns="http://www.w3.org/2000/svg">
  <rect width="1024" height="1024" fill="#7C3AED" rx="180"/>
  <ellipse cx="512" cy="480" rx="300" ry="200" fill="white"/>
  <circle cx="512" cy="480" r="120" fill="#7C3AED"/>
</svg>
EOF
            # Try to use rsvg-convert if available, otherwise give instructions
            if command -v rsvg-convert &> /dev/null; then
                rsvg-convert -w 1024 -h 1024 temp_base.svg -o temp_base.png
                rm temp_base.svg
            else
                echo "Cannot create icon automatically. Please install ImageMagick:"
                echo "  brew install imagemagick"
                exit 1
            fi
        }
    else
        echo "No source icon found and no image tools available."
        echo "Please install ImageMagick: brew install imagemagick"
        exit 1
    fi
    
    # Generate all sizes
    for size in 16 32 64 128 256 512 1024; do
        sips -z $size $size temp_base.png --out "$ICONSET/icon_${size}x${size}.png" > /dev/null
        if [ $size -le 512 ]; then
            double=$((size*2))
            sips -z $double $double temp_base.png --out "$ICONSET/icon_${size}x${size}@2x.png" > /dev/null
        fi
    done
    
    rm -f temp_base.png
else
    echo "Error: Neither ImageMagick nor sips found. Cannot create icons."
    echo "Install ImageMagick: brew install imagemagick"
    exit 1
fi

echo "Converting iconset to .icns..."
iconutil -c icns "$ICONSET" -o "$ICNS"

echo "Cleaning up..."
rm -rf "$ICONSET"

echo "✅ Created $ICNS"
