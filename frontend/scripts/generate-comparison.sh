#!/usr/bin/env bash
# Synthetic illustration only. No personal data or inference model.
set -euo pipefail
cd "$(dirname "$0")/.."
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i 'color=c=0x243a39:s=960x540:r=24:d=8' \
  -f lavfi -i 'color=c=0x9bd5b7:s=40x12:r=24:d=8' \
  -filter_complex "[0:v]drawgrid=width=120:height=90:thickness=1:color=0x47615f,drawbox=x=100:y=140:w=230:h=280:color=0x9cae9a:t=fill,drawbox=x=660:y=100:w=180:h=320:color=0x748e7a:t=fill,drawbox=x=410:y=290:w=140:h=130:color=0x9daabf:t=fill,drawbox=x=410:y=140:w=140:h=150:color=0xe6bc92:t=fill,drawbox=x=410:y=140:w=140:h=28:color=0x423c35:t=fill,drawbox=x=435:y=195:w=20:h=12:color=0x242924:t=fill,drawbox=x=505:y=195:w=20:h=12:color=0x242924:t=fill,drawbox=x=455:y=250:w=50:h=9:color=0x874b40:t=fill[scene];[scene][1:v]overlay=x='mod(t*100,920)':y=470:shortest=1[out]" \
  -map '[out]' -an -c:v libx264 -pix_fmt yuv420p -movflags +faststart static/original.mp4
ffmpeg -hide_banner -loglevel error -y -i static/original.mp4 \
  -filter_complex '[0:v]split[base][face];[face]crop=140:150:410:140,boxblur=20:3[blur];[base][blur]overlay=410:140[out]' \
  -map '[out]' -an -c:v libx264 -pix_fmt yuv420p -movflags +faststart static/sample.mp4
