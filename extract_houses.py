from PIL import Image

img_path = '/Users/bruno/Downloads/18.09a - Iconic Homestead 2.1/iconic_homestead v1.png'
out_dir = '/Users/bruno/MMORPG/Assets/_Project/Art/Houses'

img = Image.open(img_path)

# Crop coordinates: (left, top, right, bottom)
# 1. Big House (Top left)
# Left: 0, Top: 0, Right: 192 (12 tiles), Bottom: 240 (15 tiles)
house1 = img.crop((0, 0, 192, 240))
house1.save(f'{out_dir}/CuteHouse_Big.png')

# 2. Small House (Far right)
# Left: 464 (col 29), Top: 64 (row 4), Right: 608 (col 38), Bottom: 240 (row 15)
house2 = img.crop((464, 48, 608, 240))
house2.save(f'{out_dir}/CuteHouse_Small.png')

# 3. Octagonal Stone House (Bottom left)
# Left: 0, Top: 320 (row 20), Right: 144 (col 9), Bottom: 464 (row 29)
house3 = img.crop((16, 336, 144, 464))
house3.save(f'{out_dir}/CuteHouse_Stone.png')

print("Houses extracted successfully!")
