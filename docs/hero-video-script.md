# Hero video shooting script

The landing-page hero video (`site/public/spotuify-demo.mp4`) and the README
gif come from this recording. Target: ~40 seconds, one continuous terminal
session, real account, real playback.

## Why this arc

TUI-led hybrid. A pure CLI video is visually indistinguishable from any other
tool's gif and buries the one visual differentiator (album art + visualizer +
synced lyrics in a terminal). A pure TUI video looks like every existing
Spotify TUI demo. The arc nobody else can record is:

> gorgeous TUI → quit it → the music doesn't care → drive it from a pipe →
> reopen, state intact.

The 43s vhs recording under "The contract" on the landing page already proves
the daemon claim in plain text; the hero's job is the same arc with real album
art and the spectrum dancing.

## Setup

- Terminal: kitty or Ghostty (kitty graphics protocol, so album art renders).
- Window: ~1400x800, or any 16:9-ish region. Font 16-18pt. Dark theme.
- Music **already playing** before you hit record, so the visualizer is alive
  from frame one. Pick a track with striking album art.
- Clean prompt (no long CWD/git segments), no notification popups, hide other
  windows behind the capture region.
- Recorder: QuickTime (File > New Screen Recording > select region) or any
  region capture. Audio doesn't matter; the page mutes the video.

## Beats

| # | Action | Hold | What it proves |
|---|--------|------|----------------|
| 1 | `spotuify` → Player screen: art, spectrum, synced lyrics scrolling | 6s | the flagship looks like a product |
| 2 | `/` → type `luther vandross` → `Enter` → `Enter` to play a result; art and theme swap | 8s | keyboard-native browsing |
| 3 | `q` → prompt returns | 2s | the exit |
| 4 | `spotuify status --format json \| jq '{playing: .is_playing, track: .item.name}'` → shows `"playing": true` | 5s | **the money shot** |
| 5 | `spotuify search 'burial' --type track --limit 5 --format ids \| spotuify queue add --ids -` → `queued 5 item(s)` | 7s | unix pipes are the product |
| 5b (optional) | re-run the same pipe → `skipped 5 already queued` | 4s | queue set semantics |
| 6 | `spotuify` → Queue rail shows the 5; progress bar further along, playback never stopped | 6s | one daemon, clients are views |
| 7 (optional) | `q` → `spotuify analytics top --kind artists --since 30d` | 5s | local Wrapped tease |

Beat 4's pipe output is the frame people screenshot. Let it breathe a full
beat before moving on.

Copy-paste block for beats 4-7:

```bash
spotuify status --format json | jq '{playing: .is_playing, track: .item.name}'
spotuify search 'burial' --type track --limit 5 --format ids | spotuify queue add --ids -
spotuify analytics top --kind artists --since 30d
```

## Post-production

1. Trim dead typing and any flubbed beats.
2. Compress (target under 6MB):

   ```bash
   ffmpeg -i raw.mov -vcodec libx264 -crf 26 -r 30 -movflags +faststart spotuify-demo.mp4
   ```

3. Replace `site/public/spotuify-demo.mp4`.
4. Regenerate the README gif from the same cut:

   ```bash
   ffmpeg -i spotuify-demo.mp4 -vf "fps=12,scale=960:-1" site/public/spotuify-demo.gif
   ```

5. Rebuild + deploy the site (`cd site && npm run build && vercel --prod`).

## Checklist before publishing

- [ ] Visualizer animating in beat 1 (music was playing before record start)
- [ ] Album art visible in the TUI (kitty graphics, not half-block fallback)
- [ ] Beat 4 shows `"playing": true` after the TUI quit
- [ ] No personal info you mind shipping (playlist names are visible)
- [ ] MP4 under ~6MB, plays muted + looped
