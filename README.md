# cosmic-hdr-tuner

Tiny iced GUI for live-tuning the HDR shader parameters in
[cosmic-comp](https://github.com/jibsta210/cosmic-comp/tree/feat/hdr-experiment)'s
experimental HDR pipeline. Reads / writes the HDR fields in
`~/.local/state/cosmic-comp/outputs.ron` and signals cosmic-comp via
`SIGUSR1` to apply changes without a full session relogin.

This exists as a stop-gap for the **Lilypad Cosmic** fork's HDR experiment —
intended to be folded into `cosmic-settings` upstream once the math has stabilized.
Filed alongside tracking issue [pop-os/cosmic-comp#1384](https://github.com/pop-os/cosmic-comp/issues/1384).

## What it tunes

| Field                | Range           | What it controls                                              |
|---------------------|-----------------|---------------------------------------------------------------|
| HDR enabled         | toggle          | Master HDR mode for the connector                             |
| Colorspace          | BT.2020 / DCI-P3 | KMS `Colorspace` property + metadata primaries + shader matrix |
| Reference white     | 80–600 cd/m²    | SDR 1.0 → linear nits scaling fed into the inverse PQ EOTF    |
| Gamut strength      | 0–100 %         | Lerp between identity and Rec.709→target matrix in shader     |
| Test pattern        | toggle          | Replace desktop with calibration quadrants (`color_mode=6.0`) |

## Required cosmic-comp branch

This GUI only works with cosmic-comp built from
[`feat/hdr-experiment`](https://github.com/jibsta210/cosmic-comp/tree/feat/hdr-experiment),
which adds:

- The HDR fields to `OutputConfig` (see `cosmic-comp-config/src/output/comp.rs`).
- A SIGUSR1 hot-reload path (`src/lib.rs` → `push_hdr_tuning_to_surfaces`).
- Custom uniforms (`hdr_colorspace`, `hdr_ref_white`, `hdr_gamut_mix`) on the
  postprocess shader (`src/backend/render/shaders/offscreen.frag`).

It also depends on a smithay fork
([feat/hdr-experiment](https://github.com/jibsta210/smithay/tree/feat/hdr-experiment))
for the Atomic-DRM HDR state staging.

## Build

```sh
cargo build --release
sudo install -m 755 target/release/cosmic-hdr-tuner /usr/local/bin/
sudo install -m 644 cosmic-hdr-tuner.desktop /usr/share/applications/
```

## License

GPL-3.0-only (matches cosmic-comp's license — this is meant to feed into upstream).
