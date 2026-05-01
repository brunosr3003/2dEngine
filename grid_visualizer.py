from PIL import Image, ImageDraw, ImageFont

# Open the image
img_path = '/Users/bruno/Downloads/18.09a - Iconic Homestead 2.1/iconic_homestead v1.png'
img = Image.open(img_path)

# Create a copy to draw on
draw = ImageDraw.Draw(img)

# Dimensions
width, height = img.size
tile_size = 16
cols = width // tile_size
rows = height // tile_size

# Draw grid and indices
# Unity slices from Bottom-Left by default? No, TextureImporter slicing by Grid (Top Left) goes Top->Bottom, Left->Right.
# Let's label them Top->Bottom, Left->Right.
index = 0
for y in range(rows):
    for x in range(cols):
        # Draw rectangle
        left = x * tile_size
        top = y * tile_size
        right = left + tile_size
        bottom = top + tile_size
        draw.rectangle([left, top, right, bottom], outline=(255, 0, 0, 128))
        
        # Draw text
        draw.text((left + 2, top + 2), str(index), fill=(255, 255, 0, 255))
        index += 1

# Save the result
output_path = '/Users/bruno/2dEngine/scratch_grid.png'
img.save(output_path)
print(f"Saved annotated grid to {output_path}")
