#!/usr/bin/env bash
# Spotr installer: installs the build dependencies, Rust, and Spotr itself.
#
#   ./install.sh                 from a clone of the repository
#   curl -fsSL https://raw.githubusercontent.com/kubos5/spotr/main/install.sh | bash
#
# Everything it adds is written to a manifest so ./uninstall.sh can undo it.
#
# Copyright (C) Spotr contributors. Licensed under the GNU GPL v3 or later.

set -u

REPO_URL="https://github.com/kubos5/spotr.git"
MIN_RUST="1.88"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/spotr"
MANIFEST="$DATA_DIR/install-manifest"
LOG="$(mktemp "${TMPDIR:-/tmp}/spotr-install.XXXXXX")"

ASSUME_YES=0
AUDIO=1
for arg in "$@"; do
    case "$arg" in
        -y|--yes) ASSUME_YES=1 ;;
        --no-audio) AUDIO=0 ;;
        -h|--help)
            cat <<'EOF'
Spotr installer

USAGE:
    ./install.sh [OPTIONS]

OPTIONS:
    -y, --yes       Don't ask for confirmation
    --no-audio      Build without the built-in player (no ALSA / audio libraries needed)
    -h, --help      Show this help

Undo everything with ./uninstall.sh
EOF
            exit 0 ;;
        *) printf 'Unknown option: %s (try --help)\n' "$arg" >&2; exit 2 ;;
    esac
done

# ───────────────────────────────────────────────────────────────── look & feel
# Only the terminal's own 16 colors are used, so the installer follows your theme,
# just like Spotr does.

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != "dumb" ]; then
    FANCY=1
    R=$'\e[0m'; B=$'\e[1m'; DIM=$'\e[2m'; IT=$'\e[3m'
    ACC=$'\e[32m'; ACCB=$'\e[1;32m'; BRIGHT=$'\e[1;92m'; MUTED=$'\e[90m'
    RED=$'\e[31m'; YEL=$'\e[33m'; GRN=$'\e[32m'; KEY=$'\e[7m'
    HIDE=$'\e[?25l'; SHOW=$'\e[?25h'; CLR=$'\e[2K'
    PALETTE=(31 33 32 36 34 35)
else
    FANCY=0
    R=; B=; DIM=; IT=; ACC=; ACCB=; BRIGHT=; MUTED=; RED=; YEL=; GRN=; KEY=; HIDE=; SHOW=; CLR=
    PALETTE=()
fi

case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
    *UTF-8*|*utf8*|*UTF8*|*utf-8*) UNI=1 ;;
    *) [ "$(locale charmap 2>/dev/null)" = "UTF-8" ] && UNI=1 || UNI=0 ;;
esac

if [ "$UNI" = 1 ]; then
    TL="╭"; TR="╮"; BL="╰"; BR="╯"; H="─"; V="│"
    OK="✓"; FAIL="✗"; ARROW="❯"; DOT="·"; NOTE="ℹ"; ELL="…"
    SPIN=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
    WAVE=("▁" "▂" "▃" "▄" "▅" "▆" "▇" "█")
else
    TL="+"; TR="+"; BL="+"; BR="+"; H="-"; V="|"
    OK="ok"; FAIL="x"; ARROW=">"; DOT="-"; NOTE="i"; ELL="~"
    SPIN=("|" "/" "-" "\\")
    WAVE=("_" "." "-" "~" "~" "^" "^" "^")
fi

COLS=$(tput cols 2>/dev/null || echo 80)
[ "$COLS" -gt 76 ] && W=76 || W=$((COLS - 2))
[ "$W" -lt 40 ] && W=40

restore() { printf '%s' "$SHOW"; stty echo 2>/dev/null </dev/tty || true; }
trap restore EXIT
trap 'restore; printf "\n  %sCancelled.%s\n\n" "$MUTED" "$R"; exit 130' INT TERM

# A UTF-8 locale, so bash counts characters rather than bytes.
UTF8LOC=$(locale -a 2>/dev/null | grep -i -m1 -E '^(C|en_US)\.utf-?8$' || true)

# A string without its color codes.
strip() {
    local s="${1//$'\e'\[[0-9;?]m/}"
    s="${s//$'\e'\[[0-9;?][0-9;?]m/}"
    s="${s//$'\e'\[[0-9;?][0-9;?][0-9;?]m/}"
    printf '%s' "${s//$'\e'\[[0-9;?][0-9;?][0-9;?][0-9;?]m/}"
}

# Printed width of a string (ignores color codes).
vlen() {
    local s
    s="$(strip "$1")"
    [ -n "$UTF8LOC" ] && local LC_ALL="$UTF8LOC"
    printf '%s' "${#s}"
}

# Cut a string to $2 columns (dropping its colors if it has to be cut).
fit() {
    if [ "$(vlen "$1")" -le "$2" ]; then printf '%s' "$1"; return; fi
    local s
    s="$(strip "$1")"
    [ -n "$UTF8LOC" ] && local LC_ALL="$UTF8LOC"
    printf '%s%s' "${s:0:$(($2 - 1))}" "$ELL"
}

repeat() { local i; for ((i = 0; i < $2; i++)); do printf '%s' "$1"; done; }

center() {
    local pad=$(( (COLS - $(vlen "$1")) / 2 ))
    [ "$pad" -lt 0 ] && pad=0
    printf '%*s%s\n' "$pad" "" "$1"
}

# box "Title" line…   — a rounded panel like Spotr's popups.
box() {
    local title="$1"; shift
    local inner=$((W - 2)) line len
    if [ -n "$title" ]; then
        len=$(vlen "$title")
        printf ' %s%s%s %s%s%s %s%s%s\n' "$MUTED" "$TL" "$H" "$ACCB" "$title" "$R" "$MUTED" "$(repeat "$H" $((inner - len - 3)))" "$TR$R"
    else
        printf ' %s%s%s%s%s\n' "$MUTED" "$TL" "$(repeat "$H" "$inner")" "$TR" "$R"
    fi
    for line in "$@"; do
        line="$(fit "$line" $((inner - 2)))"
        len=$(vlen "$line")
        printf ' %s%s%s %s%*s %s%s%s\n' "$MUTED" "$V" "$R" "$line" $((inner - 2 - len)) "" "$MUTED" "$V" "$R"
    done
    printf ' %s%s%s%s%s\n' "$MUTED" "$BL" "$(repeat "$H" "$inner")" "$BR" "$R"
}

LOGO=(
    "▄█████ █████▄ ▄████▄ ██████ █████▄ "
    "▀▀▀▄▄▄ ██▄▄█▀ ██  ██   ██   ██▄▄██▄"
    "█████▀ ██     ▀████▀   ██   ██   ██"
)
TAGLINE="a better Spotify client, for the terminal"

# The logo with a shimmer band sweeping across it (Spotr's title shimmer).
draw_logo() {
    local band=$1 row line i ch out pad
    pad=$(( (COLS - 35) / 2 )); [ "$pad" -lt 0 ] && pad=0
    for row in 0 1 2; do
        line="${LOGO[$row]}"
        out=""
        for ((i = 0; i < ${#line}; i++)); do
            ch="${line:$i:1}"
            if [ "$band" -ge 0 ] && [ $((i - band)) -ge -2 ] && [ $((i - band)) -le 2 ]; then
                out+="$BRIGHT$ch"
            else
                out+="$ACCB$ch"
            fi
        done
        printf '%s%*s%s%s\n' "$CLR" "$pad" "" "$out" "$R"
    done
}

SINE=(4 5 6 7 7 6 5 4 3 2 1 0 0 1 2 3)
wave() {
    local t=$1 n=30 i lvl col out="" pad
    pad=$(( (COLS - n) / 2 )); [ "$pad" -lt 0 ] && pad=0
    for ((i = 0; i < n; i++)); do
        lvl=${SINE[$(( (i + t) % ${#SINE[@]} ))]}
        if [ "${#PALETTE[@]}" -gt 0 ]; then
            col=${PALETTE[$(( i % ${#PALETTE[@]} ))]}
            out+=$'\e['"${col}m${WAVE[$lvl]}"
        else
            out+="${WAVE[$lvl]}"
        fi
    done
    printf '%s%*s%s%s\n' "$CLR" "$pad" "" "$out" "$R"
}

banner() {
    local subtitle="$1" f n pad
    echo
    if [ "$UNI" = 1 ]; then
        if [ "$FANCY" = 1 ]; then
            printf '%s' "$HIDE"
            n=${#TAGLINE}
            pad=$(( (COLS - n) / 2 )); [ "$pad" -lt 0 ] && pad=0
            # Shimmer + typewriter tagline + rippling wave, then settle.
            for ((f = 0; f <= 44; f++)); do
                draw_logo $((f - 4))
                printf '\n%s%*s%s%s%s\n\n' "$CLR" "$pad" "" "$DIM" "${TAGLINE:0:$(( f * n / 30 ))}" "$R"
                wave "$f"
                sleep 0.02
                printf '\e[7A'
            done
            draw_logo -10
            printf '\n'; center "$DIM$TAGLINE$R"; printf '\n'
            wave 44
            printf '%s' "$SHOW"
        else
            draw_logo -10; echo; center "$TAGLINE"
        fi
    else
        center "S P O T R"; echo; center "$TAGLINE"
    fi
    echo
    center "$MUTED$subtitle$R"
    echo
}

# step "Label" command…   — runs a command with a spinner; output goes to the log.
# Shows the latest "Compiling …" crate while cargo works.
step() {
    local label="$1"; shift
    local start=$SECONDS i=0 status detail el room
    room=$((W - $(vlen "$label") - 12))
    "$@" >>"$LOG" 2>&1 &
    local pid=$!
    if [ "$FANCY" = 1 ]; then
        printf '%s' "$HIDE"
        while kill -0 "$pid" 2>/dev/null; do
            el=$((SECONDS - start))
            detail=$(tail -n 40 "$LOG" 2>/dev/null | grep -Eo '(Compiling|Downloaded|Installing|Unpacking|Setting up|Building) [^ ]+( v[0-9.]+)?' | tail -n 1)
            printf '\r%s  %s%s%s %s  %s%s%s' "$CLR" "$ACC" "${SPIN[$((i % ${#SPIN[@]}))]}" "$R" "$label" "$MUTED" "${detail:0:$((room > 0 ? room : 0))}" "$R"
            [ "$el" -ge 5 ] && printf ' %s%ds%s' "$MUTED" "$el" "$R"
            i=$((i + 1))
            sleep 0.08
        done
        printf '%s' "$SHOW"
    else
        printf '  ... %s\n' "$label"
    fi
    wait "$pid"; status=$?
    el=$((SECONDS - start))
    if [ "$status" -eq 0 ]; then
        printf '\r%s  %s%s%s %s' "$CLR" "$GRN" "$OK" "$R" "$label"
        [ "$el" -ge 5 ] && printf ' %s%s %ds%s' "$MUTED" "$DOT" "$el" "$R"
        printf '\n'
    else
        printf '\r%s  %s%s%s %s\n\n' "$CLR" "$RED" "$FAIL" "$R" "$label"
        local tail_lines=("")
        while IFS= read -r l; do tail_lines+=("$MUTED${l:0:$((W - 6))}$R"); done < <(tail -n 12 "$LOG")
        box "${RED}Something went wrong$R" "${tail_lines[@]}" "" "Full log: $B$LOG$R"
        echo
        exit 1
    fi
}

done_line() { printf '  %s%s%s %s\n' "$GRN" "$OK" "$R" "$1"; }
info_line() { printf '  %s%s%s %s\n' "$ACC" "$NOTE" "$R" "$1"; }
warn_line() { printf '  %s!%s %s\n' "$YEL" "$R" "$1"; }

die() { echo; box "${RED}Can't continue$R" "$@"; echo; exit 1; }

# Read one key from the terminal (works under `curl | bash` too).
ask() {
    local prompt="$1" default="$2" reply
    if [ "$ASSUME_YES" = 1 ]; then return 0; fi
    if ! [ -r /dev/tty ]; then die "No terminal to ask for confirmation." "Run again with $B--yes$R to skip the questions."; fi
    printf '  %s%s%s %s ' "$ACCB" "$ARROW" "$R" "$prompt"
    IFS= read -rsn1 reply </dev/tty || reply=""
    case "$reply" in
        "") reply="$default" ;;
        $'\e'|q|Q) reply="n" ;;
    esac
    case "$reply" in
        y|Y) printf '%syes%s\n' "$ACC" "$R"; return 0 ;;
        *) printf '%sno%s\n' "$MUTED" "$R"; return 1 ;;
    esac
}

# ─────────────────────────────────────────────────────────────── detection

OS="$(uname -s)"
PM=""            # package manager
PKGS=()          # what Spotr needs from it
case "$OS" in
    Darwin) PM="xcode" ;;
    Linux)
        if command -v apt-get >/dev/null 2>&1; then
            PM="apt";    PKGS=(build-essential pkg-config git curl ca-certificates)
            [ "$AUDIO" = 1 ] && PKGS+=(libasound2-dev)
        elif command -v dnf >/dev/null 2>&1; then
            PM="dnf";    PKGS=(gcc gcc-c++ make pkgconf-pkg-config git curl)
            [ "$AUDIO" = 1 ] && PKGS+=(alsa-lib-devel)
        elif command -v pacman >/dev/null 2>&1; then
            PM="pacman"; PKGS=(base-devel pkgconf git curl)
            [ "$AUDIO" = 1 ] && PKGS+=(alsa-lib)
        elif command -v zypper >/dev/null 2>&1; then
            PM="zypper"; PKGS=(gcc gcc-c++ make pkg-config git curl)
            [ "$AUDIO" = 1 ] && PKGS+=(alsa-devel)
        elif command -v apk >/dev/null 2>&1; then
            PM="apk";    PKGS=(build-base pkgconf git curl)
            [ "$AUDIO" = 1 ] && PKGS+=(alsa-lib-dev)
        fi ;;
esac

pkg_installed() {
    case "$PM" in
        apt) dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q "ok installed" ;;
        dnf|zypper) rpm -q --whatprovides "$1" >/dev/null 2>&1 ;;
        pacman) pacman -Qi "$1" >/dev/null 2>&1 || pacman -Qg "$1" >/dev/null 2>&1 ;;
        apk) apk info -e "$1" >/dev/null 2>&1 ;;
        *) return 0 ;;
    esac
}

MISSING=()
for p in ${PKGS[@]+"${PKGS[@]}"}; do
    pkg_installed "$p" || MISSING+=("$p")
done

NEED_CLT=0
if [ "$PM" = "xcode" ] && ! xcode-select -p >/dev/null 2>&1; then NEED_CLT=1; fi

# Rust: use what's there if it's new enough.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
RUST_STATE="missing"
if command -v cargo >/dev/null 2>&1; then
    have=$(rustc --version 2>/dev/null | awk '{print $2}')
    if [ "$(printf '%s\n%s\n' "$MIN_RUST" "$have" | sort -t. -k1,1n -k2,2n -k3,3n | head -n1)" = "$MIN_RUST" ]; then
        RUST_STATE="ok"
    elif command -v rustup >/dev/null 2>&1; then
        RUST_STATE="update"
    else
        RUST_STATE="old"
    fi
fi

# Source: this clone, or a fresh one in a temporary directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"
if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/Cargo.toml" ] && grep -q '^name = "spotr"' "$SCRIPT_DIR/Cargo.toml"; then
    SRC="$SCRIPT_DIR"; CLONE=0
else
    SRC="$(mktemp -d "${TMPDIR:-/tmp}/spotr-src.XXXXXX")"; CLONE=1
fi
VERSION=$(grep -m1 '^version' "$SRC/Cargo.toml" 2>/dev/null | cut -d'"' -f2)

PREV_INSTALL=0
[ -f "$MANIFEST" ] && PREV_INSTALL=1

# ───────────────────────────────────────────────────────────────── the plan

banner "installer${VERSION:+ $DOT v$VERSION}"

if [ "$OS" = "Linux" ] && [ -z "$PM" ]; then
    die "Couldn't find a supported package manager (apt, dnf, pacman, zypper, apk)." \
        "Install a C compiler, pkg-config, git, curl and the ALSA headers," \
        "then run: ${B}cargo install --path . --locked$R"
fi
if [ "$OS" != "Linux" ] && [ "$OS" != "Darwin" ]; then
    die "This installer supports macOS and Linux." "On Windows, follow the README's \"Compile from source\" steps."
fi

plan=()
if [ "$NEED_CLT" = 1 ]; then
    plan+=("$ACC$ARROW$R Xcode Command Line Tools $MUTED(compiler, git)$R")
fi
if [ "${#MISSING[@]}" -gt 0 ]; then
    plan+=("$ACC$ARROW$R System packages $MUTED($PM)$R")
    plan+=("    $DIM${MISSING[*]}$R")
fi
case "$RUST_STATE" in
    missing) plan+=("$ACC$ARROW$R Rust $MUTED(via rustup, into ~/.cargo and ~/.rustup)$R") ;;
    update)  plan+=("$ACC$ARROW$R Update Rust to stable $MUTED(found $have, need $MIN_RUST+)$R") ;;
esac
[ "$CLONE" = 1 ] && plan+=("$ACC$ARROW$R Download the source $MUTED(temporary, removed afterwards)$R")
if [ "$PREV_INSTALL" = 1 ]; then
    plan+=("$ACC$ARROW$R Rebuild and update Spotr $MUTED(~/.cargo/bin/spotr)$R")
else
    plan+=("$ACC$ARROW$R Build and install Spotr $MUTED(~/.cargo/bin/spotr)$R")
fi
[ "$AUDIO" = 0 ] && plan+=("    ${DIM}without the built-in player (--no-audio)$R")
already=()
[ "$PM" != "xcode" ] && [ "${#PKGS[@]}" -gt "${#MISSING[@]}" ] && already+=("system packages")
[ "$PM" = "xcode" ] && [ "$NEED_CLT" = 0 ] && already+=("Xcode tools")
[ "$RUST_STATE" = "ok" ] && already+=("Rust $have")

lines=("${plan[@]}")
if [ "${#already[@]}" -gt 0 ]; then
    joined=$(printf "%s, " "${already[@]}"); joined=${joined%, }
    lines+=("" "${MUTED}Already there: $joined$R")
fi
lines+=("" "${MUTED}Everything added is recorded, so ${R}./uninstall.sh${MUTED} can undo it.$R")
box "Here's the plan" "${lines[@]}"
echo

if [ "$RUST_STATE" = "old" ]; then
    die "Found Rust $have, but Spotr needs $MIN_RUST or newer, and rustup isn't installed." \
        "Update Rust with your package manager (or remove it so rustup can be used), then try again."
fi

ask "Install now? ${MUTED}[${R}Y${MUTED}/n]$R" y || { printf '\n  %sNothing was changed.%s\n\n' "$MUTED" "$R"; exit 0; }
echo

# ──────────────────────────────────────────────────────────────── manifest
# Written as we go, so even a failed install can be undone.

new_dirs=()
d="$DATA_DIR"
while [ ! -d "$d" ]; do new_dirs+=("$d"); d="$(dirname "$d")"; done
mkdir -p "$DATA_DIR"
if [ "$PREV_INSTALL" = 0 ]; then
    { echo "# Spotr install manifest — read by uninstall.sh. Don't edit."
      echo "created=$(date '+%Y-%m-%d %H:%M:%S')"
      echo "os=$OS"
      # Directories we created (removed again if empty).
      for d in ${new_dirs[@]+"${new_dirs[@]}"}; do echo "dir=$d"; done; } >"$MANIFEST"
fi
record() { grep -qxF "$1" "$MANIFEST" 2>/dev/null || echo "$1" >>"$MANIFEST"; }

SUDO=""
if [ "$(id -u)" != 0 ]; then SUDO="sudo"; fi

# ───────────────────────────────────────────────────────────────── install

if [ "$NEED_CLT" = 1 ]; then
    info_line "A macOS dialog will ask to install the Command Line Tools. Click ${B}Install$R."
    xcode-select --install >>"$LOG" 2>&1 || true
    record "xcode_clt=1"
    wait_clt() { until xcode-select -p >/dev/null 2>&1; do sleep 3; done; }
    step "Waiting for the Command Line Tools" wait_clt
fi

if [ "${#MISSING[@]}" -gt 0 ]; then
    if [ -n "$SUDO" ]; then
        command -v sudo >/dev/null 2>&1 || die "Installing packages needs root, and sudo isn't available." "Run this as root, or install: ${MISSING[*]}"
        info_line "Installing system packages needs your password."
        sudo -v </dev/tty || die "sudo failed, so the system packages couldn't be installed."
        # Keep sudo alive while we work.
        ( while kill -0 $$ 2>/dev/null; do sudo -n true 2>/dev/null; sleep 50; done ) &
    fi
    record "pm=$PM"
    for p in "${MISSING[@]}"; do record "pkg=$p"; done
    case "$PM" in
        apt)    step "Updating package lists" $SUDO apt-get update
                step "Installing ${MISSING[*]}" $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y "${MISSING[@]}" ;;
        dnf)    step "Installing ${MISSING[*]}" $SUDO dnf install -y "${MISSING[@]}" ;;
        pacman) step "Installing ${MISSING[*]}" $SUDO pacman -S --needed --noconfirm "${MISSING[@]}" ;;
        zypper) step "Installing ${MISSING[*]}" $SUDO zypper --non-interactive install "${MISSING[@]}" ;;
        apk)    step "Installing ${MISSING[*]}" $SUDO apk add "${MISSING[@]}" ;;
    esac
fi

case "$RUST_STATE" in
    missing)
        install_rustup() {
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
        }
        record "rustup=1"
        # rustup adds itself to these; any it has to create are removed again on uninstall.
        for f in .profile .bash_profile .bashrc .zshenv .zshrc .tcshrc .config/fish/conf.d/rustup.fish; do
            [ -e "$HOME/$f" ] || record "file=$HOME/$f"
        done
        step "Installing Rust" install_rustup
        . "$HOME/.cargo/env" ;;
    update)
        step "Updating Rust" rustup update stable
        rustup default stable >>"$LOG" 2>&1 ;;
esac

if [ "$CLONE" = 1 ]; then
    step "Downloading Spotr" git clone --depth 1 "$REPO_URL" "$SRC"
    VERSION=$(grep -m1 '^version' "$SRC/Cargo.toml" | cut -d'"' -f2)
fi

features=()
[ "$AUDIO" = 0 ] && features=(--no-default-features)
record "bin=$HOME/.cargo/bin/spotr"
step "Building Spotr${VERSION:+ $VERSION} $MUTED(this takes a few minutes)$R" \
    cargo install --path "$SRC" --locked --force ${features[@]+"${features[@]}"}

[ "$CLONE" = 1 ] && rm -rf "$SRC"
rm -f "$LOG"

# ─────────────────────────────────────────────────────────────────── done

echo
final=("${GRN}${OK}${R} ${B}Spotr${VERSION:+ $VERSION} is installed.$R" "")
if command -v spotr >/dev/null 2>&1 && [ "$(command -v spotr)" = "$HOME/.cargo/bin/spotr" ] && [ "$RUST_STATE" != "missing" ]; then
    final+=("Start it with  ${KEY} spotr $R")
else
    final+=("Open a new terminal, then start it with  ${KEY} spotr $R")
    final+=("${MUTED}(or right now: ${R}~/.cargo/bin/spotr${MUTED})$R")
fi
final+=("" "${MUTED}Just looking? Try ${R}spotr --demo${MUTED} — no account needed.$R")
final+=("${MUTED}Changed your mind? ${R}./uninstall.sh${MUTED} undoes all of this.$R")
box "All set" "${final[@]}"
echo
