#!/usr/bin/env bash
# cosmic-hdr-tuner — one-click installer for the Lilypad Cosmic HDR experiment.
#
# Builds and installs:
#   - cosmic-comp-hdr      (HDR-enabled compositor fork)            → /usr/local/bin/
#   - cosmic-hdr-tuner     (live-tuning GUI)                        → /usr/local/bin/
#   - cosmic-hdr-tuner.desktop (launcher entry)                     → /usr/share/applications/
#   - cosmic-hdr.desktop   (Wayland session entry — optional)       → /usr/share/wayland-sessions/
#
# Source crates pulled into ~/.local/share/cosmic-hdr-tuner/src/ unless
# already present. Existing checkouts there are pulled+rebuilt rather
# than re-cloned (so local edits aren't lost).
#
# Usage:
#   ./install.sh                 # full install (build + system install)
#   ./install.sh --rebuild       # rebuild and reinstall binaries only (skip clone)
#   ./install.sh --no-session    # don't add the Wayland session entry
#   ./install.sh --help          # show help
#
# Uninstall: ./uninstall.sh

set -euo pipefail

# --- config ------------------------------------------------------------------
SRC_DIR="${COSMIC_HDR_SRC:-$HOME/.local/share/cosmic-hdr-tuner/src}"
BIN_DIR="/usr/local/bin"
DESKTOP_DIR="/usr/share/applications"
SESSION_DIR="/usr/share/wayland-sessions"
COSMIC_COMP_REPO="https://github.com/jibsta210/cosmic-comp.git"
COSMIC_COMP_BRANCH="feat/hdr-experiment"
SMITHAY_REPO="https://github.com/jibsta210/smithay.git"
SMITHAY_BRANCH="feat/hdr-experiment"
TUNER_REPO="https://github.com/jibsta210/cosmic-hdr-tuner.git"
TUNER_BRANCH="main"
CARGO_TARGET_DIR_DEFAULT="$HOME/.cache/cargo-target"

# --- args --------------------------------------------------------------------
REBUILD_ONLY=0
INSTALL_SESSION=1
for arg in "$@"; do
    case "$arg" in
        --rebuild) REBUILD_ONLY=1 ;;
        --no-session) INSTALL_SESSION=0 ;;
        -h|--help)
            sed -n '2,/^set -euo/p' "$0" | sed 's/^# \?//' | head -n -2
            exit 0
            ;;
        *) echo "unknown arg: $arg (use --help)"; exit 1 ;;
    esac
done

# --- pretty output -----------------------------------------------------------
RED=$'\e[31m'; GREEN=$'\e[32m'; BLUE=$'\e[34m'; YELLOW=$'\e[33m'; RESET=$'\e[0m'
log()  { printf "%s==>%s %s\n" "$BLUE"  "$RESET" "$*"; }
warn() { printf "%s==>%s %s\n" "$YELLOW" "$RESET" "$*" >&2; }
err()  { printf "%s==>%s %s\n" "$RED"   "$RESET" "$*" >&2; exit 1; }
ok()   { printf "%s==>%s %s\n" "$GREEN" "$RESET" "$*"; }

# --- preflight ---------------------------------------------------------------
log "Preflight check"

# we'll need cargo + git + sudo
for cmd in cargo git sudo; do
    command -v "$cmd" >/dev/null 2>&1 || err "missing required tool: $cmd"
done

# warn if the target DRM driver is wrong — Intel xe is the only path I've
# tested HDR signaling on. i915 should mostly work but I haven't validated.
if ! lspci -k 2>/dev/null | grep -qiE 'kernel driver in use:\s*(xe|i915)'; then
    warn "no Intel xe/i915 GPU detected. cosmic-comp's HDR signaling has only been"
    warn "tested on Intel xe + Tandem OLED. AMD/NVIDIA *may* work but is unverified."
fi

# rust toolchain check
rustc_version=$(rustc --version 2>/dev/null || true)
if [ -z "$rustc_version" ]; then
    err "rustc not found — install via rustup (https://rustup.rs)"
fi
log "  rust: $rustc_version"

# --- clone / pull ------------------------------------------------------------
mkdir -p "$SRC_DIR"

clone_or_pull() {
    local repo="$1" branch="$2" dir="$3"
    if [ -d "$dir/.git" ]; then
        log "  $dir already cloned — fetching latest $branch"
        git -C "$dir" fetch --quiet origin "$branch" || true
        git -C "$dir" checkout --quiet "$branch" || true
        git -C "$dir" pull --quiet --ff-only origin "$branch" || \
            warn "    couldn't fast-forward $branch (you have local changes?). Continuing with current state."
    else
        log "  cloning $repo into $dir"
        git clone --quiet --branch "$branch" "$repo" "$dir"
    fi
}

if [ "$REBUILD_ONLY" -eq 0 ]; then
    log "Cloning sources to $SRC_DIR"
    clone_or_pull "$COSMIC_COMP_REPO" "$COSMIC_COMP_BRANCH" "$SRC_DIR/cosmic-comp"
    clone_or_pull "$SMITHAY_REPO" "$SMITHAY_BRANCH" "$SRC_DIR/smithay"
    clone_or_pull "$TUNER_REPO" "$TUNER_BRANCH" "$SRC_DIR/cosmic-hdr-tuner"
else
    log "Rebuild-only mode: skipping clone"
    [ -d "$SRC_DIR/cosmic-comp" ] || err "$SRC_DIR/cosmic-comp missing — run without --rebuild first"
    [ -d "$SRC_DIR/smithay" ] || err "$SRC_DIR/smithay missing — run without --rebuild first"
    [ -d "$SRC_DIR/cosmic-hdr-tuner" ] || err "$SRC_DIR/cosmic-hdr-tuner missing — run without --rebuild first"
fi

# cosmic-comp uses smithay = { path = "../smithay" } — both must be siblings.
# Our $SRC_DIR layout is exactly that.

# --- build -------------------------------------------------------------------
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$CARGO_TARGET_DIR_DEFAULT}"
mkdir -p "$CARGO_TARGET_DIR"
log "Building cosmic-comp-hdr (this takes ~3-5 minutes the first time)"
log "  (target dir: $CARGO_TARGET_DIR)"
cd "$SRC_DIR/cosmic-comp"
cargo build --release || err "cosmic-comp build failed"
ok "  cosmic-comp built"

log "Building cosmic-hdr-tuner"
cd "$SRC_DIR/cosmic-hdr-tuner"
cargo build --release || err "cosmic-hdr-tuner build failed"
ok "  cosmic-hdr-tuner built"

# --- install -----------------------------------------------------------------
log "Installing binaries to $BIN_DIR (sudo)"
sudo install -m 755 "$CARGO_TARGET_DIR/release/cosmic-comp" "$BIN_DIR/cosmic-comp-hdr"
sudo install -m 755 "$CARGO_TARGET_DIR/release/cosmic-hdr-tuner" "$BIN_DIR/cosmic-hdr-tuner"

log "Installing launcher .desktop to $DESKTOP_DIR"
sudo install -m 644 "$SRC_DIR/cosmic-hdr-tuner/cosmic-hdr-tuner.desktop" \
    "$DESKTOP_DIR/cosmic-hdr-tuner.desktop"

if [ "$INSTALL_SESSION" -eq 1 ]; then
    log "Installing Wayland session entry to $SESSION_DIR"
    sudo mkdir -p "$SESSION_DIR"
    cat <<-EOF | sudo tee "$SESSION_DIR/cosmic-hdr.desktop" >/dev/null
		[Desktop Entry]
		Name=COSMIC (HDR experiment)
		Comment=Lilypad COSMIC fork with experimental HDR signaling + shader pipeline
		Exec=cosmic-comp-hdr
		Type=Application
		DesktopNames=COSMIC
	EOF
    ok "  session 'COSMIC (HDR experiment)' available on next login"
fi

# refresh desktop database so the launcher entry shows up immediately
sudo update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true

# --- done --------------------------------------------------------------------
ok ""
ok "Install complete."
ok ""
ok "  Binaries:"
ok "    $BIN_DIR/cosmic-comp-hdr"
ok "    $BIN_DIR/cosmic-hdr-tuner"
[ "$INSTALL_SESSION" -eq 1 ] && ok ""
[ "$INSTALL_SESSION" -eq 1 ] && ok "  Log out, then pick \"COSMIC (HDR experiment)\" at the greeter."
[ "$INSTALL_SESSION" -eq 1 ] && ok ""
ok "  Then launch \"COSMIC HDR Tuner\" from the app launcher and tweak."
ok ""
ok "  Disable HDR via the tuner's master toggle, or edit"
ok "    ~/.local/state/cosmic-comp-hdr/outputs.ron"
ok ""
ok "  Uninstall: run ./uninstall.sh from the same source tree."
