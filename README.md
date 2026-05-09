# cosmic-hdr-tuner

Experimental HDR support for [COSMIC](https://github.com/pop-os/cosmic-epoch) — a
forked compositor (`cosmic-comp-hdr`) that engages the panel's HDR signaling pipeline
plus an iced GUI (`cosmic-hdr-tuner`) for live-tuning the SDR-to-HDR shader.

Filed alongside the upstream HDR tracking issue:
[pop-os/cosmic-comp#1384](https://github.com/pop-os/cosmic-comp/issues/1384).

## ⚠️ Status: experimental Phase 1

This is a **research / proof-of-concept fork**, not production-ready software.
What works:

- KMS HDR signaling (`Colorspace=BT2020_RGB` or `DCI-P3_RGB_D65` + `HDR_OUTPUT_METADATA` blob + `Broadcast RGB=Full` + `max_bpc=10`)
- 10-bit `Abgr2101010` swapchain end-to-end on the primary plane
- sRGB → linear → Rec.709→target gamut → ref-white scale → ST 2084 PQ encode in a postprocess shader, applied to all desktop surfaces
- Live-tunable parameters: reference white, gamut strength, saturation, midtone gamma
- HDR session is fully isolated from vanilla cosmic-comp (own state file path, own session entry) so you can swap between regular and HDR sessions without state corruption

What's broken or missing:

- HDR-aware client surfaces (mpv with `vo=gpu-next`, HDR videos, HDR games via Proton/gamescope) — needs the Wayland color-management protocol which is still in flight upstream ([smithay#982](https://github.com/Smithay/smithay/pull/982))
- Per-surface mixed-mode HDR (vanilla cosmic-comp's planned design)
- Hardware CRTC color pipeline (KWin/Plasma 6 use the kernel's `CTM` + `GAMMA_LUT` properties for higher precision and zero shader cost; we currently do it in shader)
- Direct scanout for the postprocess output is forced off in HDR mode (otherwise the panel sees raw sRGB content tagged as PQ → washed-out). Costs ~3-8% iGPU + ~7 GB/s memory bandwidth in HDR mode only. SDR mode unaffected.

## Verified hardware

Tested only on:

- **Dell XPS 16 (DA16260)** with the 3.2K Tandem OLED panel
- **Intel Arrow Lake / Panther Lake**, `xe` driver
- **Linux 7.0+** (PSR/PSR2 disabled via `xe.enable_psr=0 xe.enable_panel_replay=0` cmdline — driver bug worked around at compositor boot, kernel fix targeted for 7.1)

Should work in principle on any HDR-capable connector with `Colorspace` enum + `HDR_OUTPUT_METADATA` blob property + 10-bit framebuffer support, but it's untested.

## One-shot install

```sh
git clone https://github.com/jibsta210/cosmic-hdr-tuner.git
cd cosmic-hdr-tuner
./install.sh
```

The installer:
1. Clones `cosmic-comp` (HDR fork) + `smithay` (HDR fork) + this repo to `~/.local/share/cosmic-hdr-tuner/src/`
2. Builds both crates with `cargo build --release` (~3-5 min on first run, much less on rebuilds — Cargo target dir cached at `~/.cache/cargo-target/`)
3. Installs binaries to `/usr/local/bin/` (sudo prompt)
4. Adds the `cosmic-hdr-tuner` launcher to `/usr/share/applications/`
5. Adds a "COSMIC (HDR experiment)" Wayland session to `/usr/share/wayland-sessions/` so it shows up at the greeter

Then log out, pick **COSMIC (HDR experiment)** at the login screen, and launch
**COSMIC HDR Tuner** from the app launcher to start tweaking.

### Install flags

```sh
./install.sh --rebuild      # rebuild + reinstall binaries; skip clone (use after editing source)
./install.sh --no-session   # don't add the Wayland session entry
```

## What the tuner controls

| Slider           | Range          | Effect                                                                                |
|------------------|----------------|---------------------------------------------------------------------------------------|
| HDR enabled      | toggle         | Master switch — engages the panel's HDR mode                                          |
| Colorspace       | BT.2020 / DCI-P3 | KMS `Colorspace` enum + metadata primaries + shader matrix target                  |
| Reference white  | 80–1000 nits   | SDR 1.0 → linear nits scaling before PQ encode. Panel ABL clamps above ~525 nits.     |
| Gamut strength   | 0–100 %        | Lerp between identity (panel does its own gamut decode) and Rec.709→target matrix in shader |
| Saturation       | 50–200 %       | Luminance-preserving chroma boost in BT.2020 luma-weighted space                       |
| Midtone gamma    | 30–200 %       | `Y^gamma` curve in luminance space. >100% darkens midtones (more contrast = HDR punch); <100% lifts midtones (looks washed) |
| Test pattern     | toggle         | Replace desktop with 4-quadrant calibration grid (color_mode=6.0)                     |

Saving in the tuner sends `SIGUSR1` to `cosmic-comp-hdr`, which re-reads outputs.ron
and pushes the new shader uniforms live to the GPU — no relogin required.

## Sensible starting values

For a ~525 nit OLED with DCI-P3-ish native gamut (Tandem OLEDs in modern XPS / ASUS / etc.):

- Reference white: **300-500** (above 525 just signals harder, ABL clamps)
- Gamut strength: **0%** (panel firmware handles gamut conversion correctly when given BT.2020-tagged content; pre-converting in shader double-converts and desaturates)
- Saturation: **120-140%** (compensates for SDR mode's vendor saturation tweaks that go away in HDR)
- Midtone gamma: **130-160%** (pushes contrast into the higher PQ range; SDR-on-HDR will look "calmer" without this)
- Colorspace: **BT.2020** (canonical HDR10 transmission; panel firmware decodes BT.2020 + PQ → maps to native gamut)

## Uninstall

```sh
cd ~/.local/share/cosmic-hdr-tuner/src/cosmic-hdr-tuner
./uninstall.sh
```

By default this removes the installed binaries + .desktop files but preserves your
HDR-session state (`~/.local/state/cosmic-comp-hdr/`) and the cloned source. Use
`--purge-state` and/or `--purge-source` to also remove those.

## License

GPL-3.0-only (matches cosmic-comp + smithay). Forks: cosmic-comp@feat/hdr-experiment,
smithay@feat/hdr-experiment.
