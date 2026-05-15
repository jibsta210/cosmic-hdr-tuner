// cosmic-hdr-tuner: a tiny iced GUI that edits the HDR fields in
// ~/.local/state/cosmic-comp/outputs.ron. Sliders for reference white +
// gamut strength, dropdown for colorspace, toggles for HDR master and
// test-pattern. Saving rewrites outputs.ron; cosmic-comp picks up the
// change on the next session apply (log out / log in for now).
//
// Designed to live next to cosmic-altswitcher in the Lilypad Cosmic fork
// set — small, standalone, intended to be subsumed into cosmic-settings
// upstream once the math has stabilized.

use cosmic_comp_config::output::comp::{
    HdrColorspace, OutputConfig, OutputsConfig, load_outputs,
};
use cosmic_comp_config::output::comp::OutputInfo;
use iced::widget::{button, checkbox, column, container, pick_list, row, slider, text};
use iced::{Element, Length, Task};
use std::path::PathBuf;

const REF_WHITE_MIN: u32 = 80;
// 10000 = full PQ peak. Most panels can't actually emit anywhere near that
// (Tandem OLED ~1000 sustained / ~1500 peak boost), but the math is well
// defined all the way up — anything past panel ABL ceiling just gets clamped
// in firmware with no visual benefit, but no harm either. Letting the user
// dial all the way to 10000 means they can max the panel without artificial
// software ceilings getting in the way.
const REF_WHITE_MAX: u32 = 10000;
// Slider step in nits. With min=80, max=10000, step=25 → ~398 stops, smooth
// enough but not jittery.
const REF_WHITE_STEP: u32 = 25;
const GAMUT_MIN: u8 = 0;
const GAMUT_MAX: u8 = 100;
const SAT_MIN: u8 = 50;
const SAT_MAX: u8 = 200;
const GAMMA_MIN: u8 = 30;
// Path B's path_b_active=1 postprocess skips the sRGB-decode + matrix +
// ref_white_scale pre-stages (already done per-surface), so user-perceived
// gamma is applied directly to the BT.2020-linear absolute-luminance values.
// SDR-white sits at PQ-aligned 1.0 (= ref_white/10000 = 1.0 at ref_white=10000),
// so a higher gamma compresses midtones harder than the hardware path's
// 0.5× dampened curve allowed. Bumped to 300% to give enough headroom for
// "punchy" contrast (50% gray at ~25 nits, white at panel peak ≈ 22:1 ratio).
// Hardware path / Phase 2A.2 effectively gets gamma * 0.5 dampening so its
// useful range stays in 30-160; users will discover via the slider that
// path B needs higher values. Capped at u8::MAX-5 = 250 (= 2.5×).
const GAMMA_MAX: u8 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorspaceUi {
    Bt2020,
    DciP3,
}

/// Convert linear-light nits in the PQ reference range (0..=10000) into the
/// PQ-encoded percentage that the panel will see on the wire. Useful as a
/// readout for "what fraction of PQ peak am I asking for?" since the slider
/// itself is in linear nits and the encoded value is what actually reaches
/// the panel and decides perceived brightness.
///
/// SMPTE ST 2084 inverse EOTF (the encoder direction):
///   Y_norm = nits / 10000
///   Y_pq   = ((c1 + c2 * Y_norm^m1) / (1 + c3 * Y_norm^m1))^m2
fn nits_to_pq_percent(nits: u32) -> f64 {
    const M1: f64 = 0.1593017578125;
    const M2: f64 = 78.84375;
    const C1: f64 = 0.8359375;
    const C2: f64 = 18.8515625;
    const C3: f64 = 18.6875;
    let y = (nits as f64 / 10000.0).max(0.0).min(1.0);
    let ym = y.powf(M1);
    let v = ((C1 + C2 * ym) / (1.0 + C3 * ym)).max(0.0).powf(M2);
    v * 100.0
}

impl std::fmt::Display for ColorspaceUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ColorspaceUi::Bt2020 => "BT.2020",
            ColorspaceUi::DciP3 => "DCI-P3 D65",
        })
    }
}

impl ColorspaceUi {
    const ALL: [ColorspaceUi; 2] = [ColorspaceUi::Bt2020, ColorspaceUi::DciP3];

    fn from_config(c: Option<HdrColorspace>) -> Self {
        match c {
            Some(HdrColorspace::DciP3) => ColorspaceUi::DciP3,
            _ => ColorspaceUi::Bt2020,
        }
    }

    fn to_config(self) -> HdrColorspace {
        match self {
            ColorspaceUi::Bt2020 => HdrColorspace::Bt2020,
            ColorspaceUi::DciP3 => HdrColorspace::DciP3,
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    HdrEnabled(bool),
    Colorspace(ColorspaceUi),
    RefWhite(u32),
    GamutStrength(u8),
    Saturation(u8),
    MidtoneGamma(u8),
    TestPattern(bool),
    Save,
    Saved(Result<(), String>),
}

struct App {
    path: PathBuf,
    config: OutputsConfig,
    /// Index in the (single) HashMap entry we're editing — keep it simple,
    /// edit the FIRST output info group that contains an HDR-capable connector.
    target_key: Option<Vec<OutputInfo>>,
    target_idx: usize,
    /// Edit-buffer of the HDR knobs we expose.
    hdr_enabled: bool,
    colorspace: ColorspaceUi,
    ref_white: u32,
    gamut_strength: u8,
    saturation: u8,
    midtone_gamma: u8,
    test_pattern: bool,
    status: String,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        // Session-isolated state: read/write cosmic-comp-hdr/outputs.ron so
        // vanilla cosmic-comp can't round-trip-corrupt our HDR state. On
        // first run, fall back to vanilla's outputs.ron if the HDR file
        // doesn't exist yet (mirrors what cosmic-comp-hdr does on startup).
        let xdg = xdg::BaseDirectories::new().expect("xdg base dirs");
        let path = xdg
            .place_state_file("cosmic-comp-hdr/outputs.ron")
            .expect("state path");
        if !path.exists() {
            if let Ok(vanilla) = xdg.place_state_file("cosmic-comp/outputs.ron") {
                if vanilla.exists() {
                    let _ = std::fs::copy(&vanilla, &path);
                }
            }
        }
        let config = load_outputs(Some(&path));

        // Find the FIRST REAL panel in outputs.ron — skip X11-*, WL-*, virtual
        // and fallback connectors that show up when running with non-KMS
        // backends (X11 backend, winit-in-X). HashMap iteration order is
        // unspecified, so without filtering we'd sometimes target the virtual
        // output and the user's saves would never reach their actual panel.
        // Match priority: prefer eDP-* / DP-* / HDMI-* etc. Fall back to first
        // entry only if no real connector exists (rare).
        let pick_real = |config: &OutputsConfig| -> Option<(Vec<OutputInfo>, usize, OutputConfig)> {
            config.config.iter().find_map(|(k, v)| {
                let info = k.first()?;
                let connector = &info.connector;
                let is_virtual = connector.starts_with("X11-")
                    || connector.starts_with("WL-")
                    || connector.starts_with("Virtual-")
                    || connector.starts_with("HEADLESS-");
                if !is_virtual {
                    Some((k.clone(), 0usize, v.first().cloned().unwrap_or_default()))
                } else {
                    None
                }
            })
        };
        let (target_key, target_idx, current): (Option<Vec<OutputInfo>>, usize, Option<OutputConfig>) =
            pick_real(&config)
                .map(|(k, i, c)| (Some(k), i, Some(c)))
                .or_else(|| {
                    // No real panel — fall back to any entry (e.g. running tuner
                    // standalone before any HDR session has populated outputs.ron).
                    config
                        .config
                        .iter()
                        .next()
                        .map(|(k, v)| (Some(k.clone()), 0usize, v.first().cloned()))
                })
                .unwrap_or((None, 0usize, None));

        let cur: OutputConfig = current.unwrap_or_else(OutputConfig::default);

        (
            Self {
                path,
                config,
                target_key,
                target_idx,
                hdr_enabled: cur.hdr_enabled.unwrap_or(false),
                colorspace: ColorspaceUi::from_config(cur.hdr_colorspace),
                ref_white: cur.hdr_reference_white.unwrap_or(250),
                gamut_strength: cur.hdr_gamut_strength.unwrap_or(100),
                saturation: cur.hdr_saturation.unwrap_or(120),
                midtone_gamma: cur.hdr_midtone_gamma.unwrap_or(100),
                test_pattern: cur.hdr_test_pattern.unwrap_or(false),
                status: String::new(),
            },
            Task::none(),
        )
    }

    fn title(&self) -> String {
        "COSMIC HDR Tuner".into()
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::HdrEnabled(v) => self.hdr_enabled = v,
            Message::Colorspace(c) => self.colorspace = c,
            Message::RefWhite(v) => self.ref_white = v.clamp(REF_WHITE_MIN, REF_WHITE_MAX),
            Message::GamutStrength(v) => self.gamut_strength = v.clamp(GAMUT_MIN, GAMUT_MAX),
            Message::Saturation(v) => self.saturation = v.clamp(SAT_MIN, SAT_MAX),
            Message::MidtoneGamma(v) => self.midtone_gamma = v.clamp(GAMMA_MIN, GAMMA_MAX),
            Message::TestPattern(v) => self.test_pattern = v,
            Message::Save => {
                let result = self.write_back();
                return Task::done(Message::Saved(result));
            }
            Message::Saved(Ok(())) => {
                // Poke cosmic-comp to re-read outputs.ron and reapply config
                // live (no relogin). Both the upstream and Lilypad-fork
                // binaries are listening; signal whichever is running.
                // -x = exact match against /proc/*/comm only (not full cmdline).
                // Earlier `-f cosmic-comp` was matching ANY process whose
                // command line contained "cosmic-comp" — including a `watch
                // grep cosmic-comp ...` from a debugging shell — and SIGUSR1
                // was killing the wrong process. Comm-exact-match is safer.
                // Process name is "cosmic-comp" (set via argv[0] in the
                // session launcher even though binary is /usr/local/bin/
                // cosmic-comp-hdr).
                let signal_result = std::process::Command::new("pkill")
                    .args(["-USR1", "-x", "cosmic-comp"])
                    .status();
                let live = matches!(signal_result, Ok(s) if s.success());
                self.status = if live {
                    format!("Saved {}. Live-reloaded.", self.path.display())
                } else {
                    format!(
                        "Saved {}. cosmic-comp not running; log into HDR session to apply.",
                        self.path.display()
                    )
                };
            }
            Message::Saved(Err(e)) => {
                self.status = format!("Save failed: {}", e);
            }
        }
        Task::none()
    }

    fn write_back(&mut self) -> Result<(), String> {
        let key = self
            .target_key
            .clone()
            .ok_or_else(|| "no output config loaded".to_string())?;
        let entry = self
            .config
            .config
            .get_mut(&key)
            .ok_or_else(|| "key disappeared".to_string())?;
        if entry.is_empty() {
            return Err("output config list empty".into());
        }
        let idx = self.target_idx.min(entry.len() - 1);
        let cfg = &mut entry[idx];
        cfg.hdr_enabled = Some(self.hdr_enabled);
        cfg.hdr_colorspace = Some(self.colorspace.to_config());
        cfg.hdr_reference_white = Some(self.ref_white);
        cfg.hdr_gamut_strength = Some(self.gamut_strength);
        cfg.hdr_saturation = Some(self.saturation);
        cfg.hdr_midtone_gamma = Some(self.midtone_gamma);
        cfg.hdr_test_pattern = Some(self.test_pattern);

        let pretty = ron::ser::PrettyConfig::default();
        let serialized = ron::ser::to_string_pretty(&self.config, pretty)
            .map_err(|e| format!("serialize: {}", e))?;
        std::fs::write(&self.path, serialized).map_err(|e| format!("write: {}", e))?;
        Ok(())
    }

    fn view(&self) -> Element<Message> {
        let target_label: String = match self.target_key.as_ref() {
            Some(k) if !k.is_empty() => {
                let info: &OutputInfo = &k[0];
                format!(
                    "Editing: {} ({} {})",
                    info.connector, info.make, info.model
                )
            }
            _ => "No output config found".to_string(),
        };

        let header = column![
            text("COSMIC HDR Tuner").size(28),
            text(target_label).size(14),
        ]
        .spacing(4);

        let master = checkbox("HDR enabled (master)", self.hdr_enabled).on_toggle(Message::HdrEnabled);

        let cs_row = row![
            text("Colorspace:"),
            pick_list(&ColorspaceUi::ALL[..], Some(self.colorspace), Message::Colorspace),
        ]
        .spacing(12)
        .align_y(iced::Alignment::Center);

        let rw_row = column![
            text(format!(
                "Reference white: {:.1}% PQ",
                nits_to_pq_percent(self.ref_white)
            )),
            slider(
                REF_WHITE_MIN..=REF_WHITE_MAX,
                self.ref_white,
                Message::RefWhite,
            )
            .step(REF_WHITE_STEP),
            text(
                "How bright sRGB white sits on the wire. PQ is non-linear: \
                ~50%=100nits, ~58%=203nits (BT.2408), ~75%=1000nits, 100%=10000nits. \
                Panel ABL clamps anything above its peak — no harm, just no extra brightness."
            )
            .size(12),
        ]
        .spacing(4);

        let gm_row = column![
            text(format!("Gamut strength: {}%", self.gamut_strength)),
            slider(
                GAMUT_MIN..=GAMUT_MAX,
                self.gamut_strength,
                Message::GamutStrength,
            )
            .step(1u8),
            text("0% = no Rec.709→target matrix (trust panel) · 100% = full conversion").size(12),
        ]
        .spacing(4);

        let sat_row = column![
            text(format!("Saturation: {}%", self.saturation)),
            slider(SAT_MIN..=SAT_MAX, self.saturation, Message::Saturation).step(5u8),
            text("100 = colorimetrically truthful · 120-140 = compensates for SDR vibrance loss")
                .size(12),
        ]
        .spacing(4);

        let mg_row = column![
            text(format!("Midtone gamma: {}%", self.midtone_gamma)),
            slider(GAMMA_MIN..=GAMMA_MAX, self.midtone_gamma, Message::MidtoneGamma).step(1u8),
            text("100 = neutral · 130-160 = HDR punch (darkens midtones, more contrast) · <100 = lifts midtones (looks washed)")
                .size(12),
        ]
        .spacing(4);

        let tp_row = checkbox(
            "Show HDR calibration test pattern (replaces desktop)",
            self.test_pattern,
        )
        .on_toggle(Message::TestPattern);

        let save = button(text("Save")).on_press(Message::Save);

        let status = text(self.status.as_str()).size(13);

        let content = column![
            header,
            master,
            cs_row,
            rw_row,
            gm_row,
            sat_row,
            mg_row,
            tp_row,
            row![save].spacing(8),
            status,
        ]
        .spacing(18)
        .padding(20)
        .max_width(640);

        container(content)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}

fn main() -> iced::Result {
    // Wayland app_id has to match the basename of the .desktop file
    // (cosmic-hdr-tuner.desktop) for the COSMIC dock to associate the
    // running window with its launcher entry. Without this, iced defaults
    // application_id to the empty string and the dock can't resolve a
    // minimized toplevel back to the launcher icon — clicking the icon
    // does nothing and the user has to use Workspace Overview to find it.
    let mut window = iced::window::Settings::default();
    window.size = iced::Size::new(720.0, 760.0);
    window.platform_specific.application_id = "cosmic-hdr-tuner".into();

    iced::application(App::title, App::update, App::view)
        // Dark theme (matches the COSMIC dark default — and the panel is
        // typically OLED HDR where light themes burn the room).
        .theme(|_| iced::Theme::Dark)
        .window(window)
        .run_with(App::new)
}
