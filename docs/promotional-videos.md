# Promotional videos

The windowsRICE promotional videos are published as a media release:

**[windowsRICE — Cinematic Product Showcase](https://github.com/mazakv/windowsRICE/releases/tag/windowsrice-showcase-2026-11-25)**
(tag `windowsrice-showcase-2026-11-25`).

This is not a software version. Application releases use `v*` tags and the
`Release` workflow. The showcase tag does not match `v*`, so it never starts
that workflow.

The video files are release assets only. They are not committed to Git. The
repository contains only the poster, at `docs/media/windowsRICE-hero-poster.png`.

## Editions

| Edition | File | Use |
|---|---|---|
| Website hero | `windowsRICE-hero.mp4`, `windowsRICE-hero.webm` | Muted background loop for a landing page. It shows the logo, "The native top bar for Windows 11" and the real bar within the first two seconds. Then come the bar in three real looks and the Design Studio's animation page. |
| Hero poster | `windowsRICE-hero-poster.png` | The poster frame for the hero video, also used in the README |
| Commercial | `windowsRICE-commercial-30s.mp4` | 30-second film: logo, introduction, three features (live restyling, animated colours, module panels), the Design Studio hero shot, and the logo ending |
| Full introduction | `windowsRICE-introduction.mp4` | 60-second film. It adds palettes and readability, RICE Design Code import, the bottom bar, and accessibility and privacy. |

Every product image in the videos is a real capture of windowsRICE. One shot in
the full introduction, the bottom bar, is an illustration assembled from real
bar captures, and it is labelled as such on screen. The music and sound effects
are original, synthesised from scratch with no samples. The hero is silent.

## Specifications

All values were measured on the published files with ffprobe, counting every
decoded frame.

| File | Video | Frames | Duration | Audio | Size |
|---|---|---|---|---|---|
| `windowsRICE-hero.mp4` | H.264 High, 1280×720, 30 fps, yuv420p, faststart | 360 | 12.000 s | none (intentional) | 1,081,856 B |
| `windowsRICE-hero.webm` | VP9 profile 0, 1280×720, 30 fps | 360 | 12.000 s | none (intentional) | 705,929 B |
| `windowsRICE-hero-poster.png` | PNG, 1920×1080, RGB | — | — | — | 1,617,413 B |
| `windowsRICE-commercial-30s.mp4` | H.264 High, 1920×1080, 60 fps | 1800 | 30.000 s | AAC-LC stereo, 48 kHz, ~320 kb/s | 18,227,552 B |
| `windowsRICE-introduction.mp4` | H.264 High, 1920×1080, 60 fps | 3600 | 60.000 s | AAC-LC stereo, 48 kHz, ~320 kb/s | 30,489,202 B |

The containers of the two films report 30.016 s and 60.011 s because AAC audio
ends on whole 1024-sample frames. The picture is exactly 30 and 60 seconds.

The hero was rendered as a 1920×1080, 60 fps, 12 s master. The 720p, 30 fps
web files are encoded from that master. The first and last frames are the same
empty background; measured on the web MP4, they differ by a mean of 0.4/255
per channel. Every ambient motion is periodic in the loop length, so the loop
has no visible seam.

## Downloads and checksums

| File | SHA-256 |
|---|---|
| [windowsRICE-hero.mp4](https://github.com/mazakv/windowsRICE/releases/download/windowsrice-showcase-2026-11-25/windowsRICE-hero.mp4) | `f0c98310f4d4503b57a4b52df27adec38bfdba4d98349dad1c7e1ec1e7cc4125` |
| [windowsRICE-hero.webm](https://github.com/mazakv/windowsRICE/releases/download/windowsrice-showcase-2026-11-25/windowsRICE-hero.webm) | `2857b2b57f467fd996b8adae9f2e65c8ffbc8f6da6d0a9ec7b7a17a40eba69ff` |
| [windowsRICE-hero-poster.png](https://github.com/mazakv/windowsRICE/releases/download/windowsrice-showcase-2026-11-25/windowsRICE-hero-poster.png) | `7e424fe8d52b9c999da9d453a6a9e93ba5b2aa1c41df5f8b734eaf13ef7ef008` |
| [windowsRICE-commercial-30s.mp4](https://github.com/mazakv/windowsRICE/releases/download/windowsrice-showcase-2026-11-25/windowsRICE-commercial-30s.mp4) | `2a6d4923edba594da5b349cc44b9215202cc0f4c3e207702fae9442bd55a9634` |
| [windowsRICE-introduction.mp4](https://github.com/mazakv/windowsRICE/releases/download/windowsrice-showcase-2026-11-25/windowsRICE-introduction.mp4) | `77077b987ca964e7f8c380e4554b5915f4c8d709f29658e8982e210cba149e28` |

`SHA256SUMS.txt` in the release lists the same values. To verify:

```sh
sha256sum -c SHA256SUMS.txt                                  # Git Bash, Linux, macOS (shasum -a 256 -c)
Get-FileHash .\windowsRICE-hero.mp4 -Algorithm SHA256        # PowerShell
```

## Website embedding

```html
<video class="hero" autoplay muted loop playsinline preload="metadata"
       poster="/media/windowsRICE-hero-poster.png" width="1280" height="720"
       aria-label="windowsRICE product preview">
  <source src="/media/windowsRICE-hero.webm" type="video/webm">
  <source src="/media/windowsRICE-hero.mp4" type="video/mp4">
</video>
```

- **Host the files yourself or on a CDN.** GitHub release downloads redirect and are served as `application/octet-stream` attachments, so they are not a reliable direct `src` for a page.
- **Mobile autoplay** needs both `muted` and `playsinline`. The hero has no audio track, so it is muted by construction.
- **List WebM first.** It is about 35% smaller. Browsers without VP9 fall back to the MP4.
- **Poster:** set `preload="metadata"` and show the poster for a fast first paint. For a lighter page, convert the PNG to WebP or AVIF at 1280×720.
- **Reduced motion:** honour `prefers-reduced-motion: reduce`. For example, show the poster instead of the video:

  ```css
  @media (prefers-reduced-motion: reduce) { video.hero { display: none; } }
  ```

- **Films:** use the 30-second commercial or the 60-second introduction with visible controls, and do not autoplay them with sound.

## Reproducing the videos

The videos come from a Remotion project (React and TypeScript), maintained
locally in `video-production/`. That project is not yet committed to this
repository. It contains:

- **Compositions:**
  - `Intro`: 60 s;
  - `Commercial30`: 30 s;
  - `Hero`: 12 s master, 1920×1080 at 60 fps.
- **Real captures** under `public/`. They are made by `tools/capture-panels.ps1` and `tools/capture-studio.ps1`, which drive the release build through UI Automation in an isolated configuration. Only windowsRICE windows are captured, over a test backdrop.
- **Score:** `tools/make-audio.mjs`. Run `node tools/make-audio.mjs` for the 60 s edition and `node tools/make-audio.mjs 30` for the 30 s edition.

Commands, from `video-production/`:

```sh
npm install
npm run audio && npm run render          # windowsRICE-introduction.mp4
npm run audio:30 && npm run render:30    # windowsRICE-commercial-30s.mp4
npx remotion render src/index.ts Hero output/windowsRICE-hero-master-1080p60.mp4 --codec=h264 --crf=14 --muted
npx remotion still src/index.ts Hero output/windowsRICE-hero-poster.png --frame=156 --image-format=png
# web encodes from the master (Remotion's bundled ffmpeg)
npx remotion ffmpeg -i output/windowsRICE-hero-master-1080p60.mp4 -an -vf scale=1280:720:flags=lanczos -r 30 -frames:v 360 \
  -c:v libx264 -profile:v high -preset slow -crf 24 -pix_fmt yuv420p -g 60 -movflags +faststart output/windowsRICE-hero.mp4
npx remotion ffmpeg -i output/windowsRICE-hero-master-1080p60.mp4 -an -vf scale=1280:720:flags=lanczos -r 30 -frames:v 360 \
  -c:v libvpx-vp9 -b:v 0 -crf 36 -deadline good -cpu-used 2 -row-mt 1 -pix_fmt yuv420p -g 60 output/windowsRICE-hero.webm
```

The project's `docs/` folder records the storyboards, the claims check behind
every on-screen statement, and the fixes made after frame inspection.

The score generator includes random pad phases, so re-rendered films are
visually identical but not bit-identical to the published files. The
checksums above identify the published files.
