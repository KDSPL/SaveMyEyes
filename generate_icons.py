"""Generate eye icon for SaveMyEyes app"""
from PIL import Image, ImageDraw
import os

def create_eye_icon(size):
    """Create an eye icon at the specified size"""
    # Create image with transparent background
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    
    # Calculate dimensions based on size
    padding = size * 0.1
    center_x = size / 2
    center_y = size / 2
    
    # Eye shape dimensions
    eye_width = size - (padding * 2)
    eye_height = eye_width * 0.5
    
    # Draw outer eye shape (almond shape using ellipse + polygon)
    # Main eye background - purple color (#8B5CF6)
    purple = (139, 92, 246, 255)
    dark_purple = (109, 40, 217, 255)
    white = (255, 255, 255, 255)
    dark = (30, 30, 50, 255)
    
    # Draw eye shape as an ellipse
    eye_bbox = [
        padding, 
        center_y - eye_height/2,
        size - padding,
        center_y + eye_height/2
    ]
    draw.ellipse(eye_bbox, fill=purple, outline=dark_purple, width=max(1, size//32))
    
    # Draw iris (white circle)
    iris_radius = eye_height * 0.4
    iris_bbox = [
        center_x - iris_radius,
        center_y - iris_radius,
        center_x + iris_radius,
        center_y + iris_radius
    ]
    draw.ellipse(iris_bbox, fill=white)
    
    # Draw pupil (dark circle)
    pupil_radius = iris_radius * 0.5
    pupil_bbox = [
        center_x - pupil_radius,
        center_y - pupil_radius,
        center_x + pupil_radius,
        center_y + pupil_radius
    ]
    draw.ellipse(pupil_bbox, fill=dark)
    
    # Draw highlight (small white circle)
    highlight_radius = pupil_radius * 0.3
    highlight_offset = pupil_radius * 0.3
    highlight_bbox = [
        center_x - highlight_offset - highlight_radius,
        center_y - highlight_offset - highlight_radius,
        center_x - highlight_offset + highlight_radius,
        center_y - highlight_offset + highlight_radius
    ]
    draw.ellipse(highlight_bbox, fill=white)
    
    return img

def main():
    icons_dir = r"C:\Users\Enthukutlet\Documents\SaveMyEyes\src-tauri\icons"
    
    # Standard sizes needed for Tauri
    sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
        "StoreLogo.png": 50,
    }
    
    for filename, size in sizes.items():
        icon = create_eye_icon(size)
        path = os.path.join(icons_dir, filename)
        icon.save(path, "PNG")
        print(f"Created {filename}")
    
    # Create ICO file (Windows icon with multiple sizes)
    ico_sizes = [16, 32, 48, 64, 128, 256]
    ico_images = [create_eye_icon(s) for s in ico_sizes]
    ico_path = os.path.join(icons_dir, "icon.ico")
    ico_images[0].save(ico_path, format='ICO', sizes=[(s, s) for s in ico_sizes], append_images=ico_images[1:])
    print("Created icon.ico")
    
    # Create ICNS file for macOS (just save as PNG, Tauri will convert)
    icns_icon = create_eye_icon(512)
    icns_path = os.path.join(icons_dir, "icon.icns")
    # ICNS requires specific handling, just copy the 512px PNG
    icns_icon.save(os.path.join(icons_dir, "icon_512.png"), "PNG")
    print("Created icon_512.png (for ICNS conversion)")
    
    print("\nAll icons generated successfully!")

if __name__ == "__main__":
    main()
