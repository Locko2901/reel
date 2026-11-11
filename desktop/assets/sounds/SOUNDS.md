# The cues

`save.wav` (1.07 s, "clip captured": two rising notes, E5 then B5), `uploaded.wav` (0.48 s,
"clip is safe on the server": one soft A5) and `session.wav` (0.62 s, "a session recording
starts, or still runs": G5 twice, short) are reel's own, synthesised with ffmpeg and under reel's
MIT licence. The cue "link copied" plays `uploaded.wav` at a lower volume by default. 44.1 kHz,
16 bit stereo PCM, as the app's player expects.

They were made with:

```sh
A1="(sin(2*PI*659.25*t)+0.3*sin(2*PI*1318.5*t)+0.08*sin(2*PI*1977.75*t))*exp(-5.5*t)*min(1,t/0.004)"
A2="if(gte(t,0.13),(sin(2*PI*987.77*(t-0.13))+0.3*sin(2*PI*1975.54*(t-0.13))+0.08*sin(2*PI*2963.3*(t-0.13)))*exp(-5*(t-0.13))*min(1,(t-0.13)/0.004),0)"
ffmpeg -f lavfi -i "aevalsrc=exprs='0.42*($A1+$A2)|0.42*($A1+$A2)':s=44100:d=1.07" \
  -af "afade=t=out:st=0.95:d=0.12,volume=-11dB" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -c:a pcm_s16le save.wav
B="(sin(2*PI*880*t)+0.35*sin(2*PI*1760*t)+0.1*sin(2*PI*2640*t))*exp(-11*t)*min(1,t/0.003)"
ffmpeg -f lavfi -i "aevalsrc=exprs='0.55*($B)|0.55*($B)':s=44100:d=0.48" \
  -af "afade=t=out:st=0.4:d=0.08,volume=-6dB" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -c:a pcm_s16le uploaded.wav
C="(sin(2*PI*783.99*t)+0.3*sin(2*PI*1567.98*t)+0.08*sin(2*PI*2351.97*t))*exp(-9*t)*min(1,t/0.003)"
D="if(gte(t,0.17),(sin(2*PI*783.99*(t-0.17))+0.3*sin(2*PI*1567.98*(t-0.17))+0.08*sin(2*PI*2351.97*(t-0.17)))*exp(-9*(t-0.17))*min(1,(t-0.17)/0.003),0)"
ffmpeg -f lavfi -i "aevalsrc=exprs='0.5*($C+$D)|0.5*($C+$D)':s=44100:d=0.62" \
  -af "afade=t=out:st=0.52:d=0.1,volume=-9dB" -map_metadata -1 -fflags +bitexact -flags:a +bitexact -c:a pcm_s16le session.wav
```

Settings, Sounds in the app takes a WAV of your own for each cue instead.
