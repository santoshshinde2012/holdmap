#!/bin/bash
# Encodes recorded hero frames (scripts/record-hero.mjs) into public/hero/*.{webm,mp4} and the
# poster in src/assets/hero/. The last 0.9s dissolve (eased, with a soft blur at the midpoint) into
# the first frame, so the video's last frame is its first and the loop has no seam.
#   scripts/encode-hero.sh <desktop|mobile> <dark|light> [x264-crf] [av1-crf]
# Needs ffmpeg with libx264 and SVT-AV1, and Python with Pillow.
set -e
V=$1; T=$2; C=${3:-20}; A=${4:-36}
D=.hero-frames/$V-$T
S=$([ "$V" = desktop ] && echo 1728:1080 || echo 1050:1470)
FPS=$(python3 -c "import json;print(json.load(open('$D/meta.json'))['FPS'])")
python3 - "$D" <<'PY'
import json, math, os, sys
from PIL import Image, ImageFilter
d = sys.argv[1]
meta = json.load(open(f"{d}/meta.json"))
n, fps, dpr = meta["frames"], meta["FPS"], meta["DPR"]
x = round(0.9 * fps)
seq = f"{d}/seq"
os.makedirs(seq, exist_ok=True)
for f in os.listdir(seq): os.remove(f"{seq}/{f}")
first = Image.open(f"{d}/00000.png").convert("RGB")
ease = lambda t: 4 * t ** 3 if t < 0.5 else 1 - (-2 * t + 2) ** 3 / 2
for i in range(n):
    name = f"{i:05d}.png"
    k = i - (n - x)
    if k < 0:
        os.symlink(os.path.abspath(f"{d}/{name}"), f"{seq}/{name}")
        continue
    e = ease((k + 1) / x)
    s = 2 * dpr * math.sin(math.pi * e)
    a = Image.open(f"{d}/{name}").convert("RGB")
    blur = (lambda im: im.filter(ImageFilter.GaussianBlur(s))) if s > 0.05 else (lambda im: im)
    Image.blend(blur(a), blur(first), e).save(f"{seq}/{name}", compress_level=1)
PY
IN="-framerate $FPS -i $D/seq/%05d.png"
F="scale=$S:flags=lanczos,format=yuv420p"
O=public/hero/hero-$V-$T
ffmpeg -y -v error $IN -vf "$F" -c:v libx264 -preset veryslow -tune animation -crf $C -profile:v high -level 5.1 -g $((FPS*4)) -bf 3 -movflags +faststart -an $O.mp4
ffmpeg -y -v error $IN -vf "$F" -c:v libsvtav1 -preset 4 -crf $A -g $((FPS*4)) -svtav1-params tune=0 -an $O.webm 2>/dev/null
ffmpeg -y -v error -i $D/00000.png -vf scale=$S:flags=lanczos src/assets/hero/hero-poster-$V-$T.png
ls -la $O.mp4 $O.webm src/assets/hero/hero-poster-$V-$T.png | awk '{print $5, $9}'
