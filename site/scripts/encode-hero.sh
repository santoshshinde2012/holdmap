#!/bin/bash
# Encodes recorded hero frames (scripts/record-hero.mjs) into public/hero/*.{webm,mp4} and the
# poster in src/assets/hero/, with a 0.6s crossfade back to the first frame so the loop is seamless.
#   scripts/encode-hero.sh <desktop|mobile> <dark|light> [x264-crf] [av1-crf]   (needs ffmpeg with SVT-AV1)
set -e
V=$1; T=$2; C=${3:-20}; A=${4:-36}
D=.hero-frames/$V-$T
FPS=$(python3 -c "import json;print(json.load(open('$D/meta.json'))['FPS'])")
N=$(python3 -c "import json;print(json.load(open('$D/meta.json'))['frames'])")
OFF=$(python3 -c "print(round($N/$FPS-0.6,3))")
if [ $V = desktop ]; then S=2240:1260; else S=1050:1470; fi
F="[0:v]fps=$FPS,format=rgb24[a];[1:v]fps=$FPS,format=rgb24[b];[a][b]xfade=transition=fade:duration=0.6:offset=$OFF,scale=$S:flags=lanczos,format=yuv420p[v]"
IN="-framerate $FPS -i $D/%05d.png -loop 1 -framerate $FPS -t 0.6 -i $D/00000.png"
O=public/hero/hero-$V-$T
ffmpeg -y -v error $IN -filter_complex "$F" -map "[v]" -c:v libx264 -preset veryslow -tune animation -crf $C -profile:v high -level 5.1 -g $((FPS*4)) -bf 3 -movflags +faststart -an $O.mp4
ffmpeg -y -v error $IN -filter_complex "$F" -map "[v]" -c:v libsvtav1 -preset 4 -crf $A -g $((FPS*4)) -svtav1-params tune=0 -an $O.webm 2>/dev/null
ffmpeg -y -v error -i $D/00000.png -vf scale=$S:flags=lanczos src/assets/hero/hero-poster-$V-$T.png
ls -la $O.mp4 $O.webm src/assets/hero/hero-poster-$V-$T.png | awk '{print $5, $9}'
