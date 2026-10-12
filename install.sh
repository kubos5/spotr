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

# Overridable for forks (and for testing the installer).
REPO_URL="${SPOTR_REPO:-https://github.com/kubos5/spotr.git}"
BRANCH="${SPOTR_BRANCH:-main}"
RELEASES="${SPOTR_RELEASES:-https://github.com/kubos5/spotr/releases}"
MIN_RUST="1.88"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/spotr"
MANIFEST="$DATA_DIR/install-manifest"
LOG="$(mktemp "${TMPDIR:-/tmp}/spotr-install.XXXXXX")"

ASSUME_YES=0
AUDIO=1
FROM_SOURCE=0
for arg in "$@"; do
    case "$arg" in
        -y|--yes) ASSUME_YES=1 ;;
        --no-audio) AUDIO=0 ;;
        --from-source) FROM_SOURCE=1 ;;
        -h|--help)
            cat <<'EOF'
Spotr installer

USAGE:
    ./install.sh [OPTIONS]

OPTIONS:
    -y, --yes       Don't ask for confirmation
    --no-audio      Build without the built-in player (no ALSA / audio libraries needed)
    --from-source   Always compile, even when a prebuilt release is available
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
    BAR_ON="━"; BAR_KNOB="●"; BAR_OFF="─"
else
    TL="+"; TR="+"; BL="+"; BR="+"; H="-"; V="|"
    OK="ok"; FAIL="x"; ARROW=">"; DOT="-"; NOTE="i"; ELL="~"
    SPIN=("|" "/" "-" "\\")
    WAVE=("_" "." "-" "~" "~" "^" "^" "^")
    BAR_ON="="; BAR_KNOB=">"; BAR_OFF="-"
fi

# Terminal width. Ask the terminal itself: `tput` falls back to 80 when its
# output isn't the terminal (as inside $(…) with stderr silenced).
COLS=$( { stty size </dev/tty; } 2>/dev/null | awk '{print $2}')
case "$COLS" in ''|*[!0-9]*|0) COLS=$(tput cols 2>/dev/tty || echo 80) ;; esac
case "$COLS" in ''|*[!0-9]*|0) COLS=80 ;; esac
[ "$COLS" -gt 76 ] && W=76 || W=$((COLS - 2))
[ "$W" -lt 40 ] && W=40
# The banner is centered over the content (the panels below it), not the whole
# terminal, so on a wide terminal it stays above the text instead of drifting right.
SPAN=$((W + 4)); [ "$SPAN" -gt "$COLS" ] && SPAN=$COLS

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
    local pad=$(( (SPAN - $(vlen "$1")) / 2 ))
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
    pad=$(( (SPAN - 35) / 2 )); [ "$pad" -lt 0 ] && pad=0
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
    local t=$1 n=32 i lvl col out="" pad
    # Centered under the logo: anchored to it rather than to the screen, so it
    # stays put whether the terminal width is odd or even. The logo is 35 wide
    # (its top row 34); the wave leaves one column on each side of the top row.
    pad=$(( (SPAN - 35) / 2 + 1 )); [ "$pad" -lt 0 ] && pad=0
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
            pad=$(( (SPAN - n) / 2 )); [ "$pad" -lt 0 ] && pad=0
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

# Overall progress: each step has a weight (roughly its expected seconds).
TOTAL_W=0   # sum of the weights of every step in the plan; 0 = no progress bar
DONE_W=0

# How far the running step is, in per mille. Cargo's own unit counter ("Building
# [==> ] 123/456") is exact; anything else is estimated from the time it usually takes.
step_progress() {
    local el=$1 expect=$2 nm p
    nm=$(tail -c 4000 "$LOG" 2>/dev/null | LC_ALL=C tr '\r' '\n' 2>/dev/null | LC_ALL=C grep -aEo 'Building \[[^]]*\] [0-9]+/[0-9]+' | tail -n 1)
    nm=${nm##* }
    if [ -n "$nm" ] && [ "${nm#*/}" -gt 0 ] 2>/dev/null; then
        # Units are counted equally, but the last one (Spotr itself) is the
        # biggest, so leave the final stretch of the bar for it.
        p=$(( ${nm%/*} * 900 / ${nm#*/} ))
    else
        p=$(( el * 1000 / (el + expect + 1) ))
    fi
    [ "$p" -gt 990 ] && p=990
    printf '%s' "$p"
}

# A Spotr-style progress bar: ━━━━━●────── 42%
bar() {
    local pm=$1 w=$((W - 12)) on
    on=$(( pm * w / 1000 ))
    printf '    %s%s%s%s%s%s%s %s%3d%%%s' "$ACC" "$(repeat "$BAR_ON" "$on")" "$BRIGHT" "$BAR_KNOB" "$MUTED" \
        "$(repeat "$BAR_OFF" $((w - on)))" "$R" "$B" $((pm / 10)) "$R"
}

# step WEIGHT "Label" command…   — runs a command with a spinner and the overall
# progress bar; output goes to the log. Shows the crate cargo is compiling.
step() {
    local weight="$1" label="$2"; shift 2
    local start=$SECONDS i=0 status detail el room p=0 q overall
    room=$((W - $(vlen "$label") - 12))
    "$@" >>"$LOG" 2>&1 &
    local pid=$!
    if [ "$FANCY" = 1 ]; then
        printf '%s' "$HIDE"
        while kill -0 "$pid" 2>/dev/null; do
            el=$((SECONDS - start))
            detail=$(tail -n 40 "$LOG" 2>/dev/null | LC_ALL=C tr '\r' '\n' 2>/dev/null | LC_ALL=C grep -aEo '(Compiling|Downloaded|Installing|Unpacking|Setting up) [^ ]+( v[0-9.]+)?' | tail -n 1)
            printf '\r%s  %s%s%s %s  %s%s%s' "$CLR" "$ACC" "${SPIN[$((i % ${#SPIN[@]}))]}" "$R" "$label" "$MUTED" "${detail:0:$((room > 0 ? room : 0))}" "$R"
            [ "$el" -ge 5 ] && printf ' %s%ds%s' "$MUTED" "$el" "$R"
            if [ "$TOTAL_W" -gt 0 ]; then
                # Never move backwards (estimates hand over to cargo's exact count).
                q=$(step_progress "$el" "$weight"); [ "$q" -gt "$p" ] && p=$q
                overall=$(( (DONE_W * 1000 + weight * p) / TOTAL_W ))
                printf '\n%s%s\e[1A' "$CLR" "$(bar "$overall")"
            fi
            i=$((i + 1))
            sleep 0.08
        done
        printf '%s' "$SHOW"
    else
        printf '  ... %s\n' "$label"
    fi
    wait "$pid"; status=$?
    el=$((SECONDS - start))
    DONE_W=$((DONE_W + weight))
    if [ "$status" -eq 0 ]; then
        printf '\r%s  %s%s%s %s' "$CLR" "$GRN" "$OK" "$R" "$label"
        [ "$el" -ge 5 ] && printf ' %s%s %ds%s' "$MUTED" "$DOT" "$el" "$R"
        printf '\n%s' "$CLR"
        # Keep the bar under the finished steps until the next one redraws it.
        [ "$FANCY" = 1 ] && [ "$TOTAL_W" -gt 0 ] && printf '%s\r' "$(bar $(( DONE_W * 1000 / TOTAL_W )))"
    else
        printf '\r%s  %s%s%s %s\n%s\n' "$CLR" "$RED" "$FAIL" "$R" "$label" "$CLR"
        local tail_lines=("")
        while IFS= read -r l; do tail_lines+=("$MUTED${l:0:$((W - 6))}$R"); done < <(tail -n 12 "$LOG" | LC_ALL=C tr '\r' '\n' 2>/dev/null | LC_ALL=C grep -av 'Building \[' | tail -n 12)
        box "${RED}Something went wrong$R" "${tail_lines[@]}" "" "Full log: $B$LOG$R"
        echo
        exit 1
    fi
}

# Clear the progress bar left under the last step.
end_steps() { [ "$FANCY" = 1 ] && printf '\r%s' "$CLR"; return 0; }

done_line() { printf '  %s%s%s %s\n' "$GRN" "$OK" "$R" "$1"; }
info_line() { printf '\r%s  %s%s%s %s\n' "$CLR" "$ACC" "$NOTE" "$R" "$1"; }
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

# Source: this clone (built in its own target/ folder), or a checkout kept in a
# cache folder. Either way, builds are incremental: on later runs (updates), only
# what changed is compiled again, not all ~400 dependencies.
if [ "$OS" = "Darwin" ]; then CACHE="$HOME/Library/Caches/spotr-install"; else CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/spotr-install"; fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd || echo "")"
if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/Cargo.toml" ] && grep -q '^name = "spotr"' "$SCRIPT_DIR/Cargo.toml"; then
    SRC="$SCRIPT_DIR"; CLONE=0; TARGET_DIR="$SRC/target"
else
    SRC="$CACHE/src"; CLONE=1; TARGET_DIR="$CACHE/target"
fi
VERSION=$(grep -m1 '^version' "$SRC/Cargo.toml" 2>/dev/null | cut -d'"' -f2)
WARM=0   # a previous build to reuse
[ -d "$TARGET_DIR/local/deps" ] && WARM=1

# Prebuilt binaries (from GitHub releases) skip compiling altogether. Used when
# installing straight from GitHub; a local clone is always built from its source.
PREBUILT=""
release_target() {
    local arch
    case "$(uname -m)" in
        arm64|aarch64) arch=aarch64 ;;
        x86_64|amd64) arch=x86_64 ;;
        *) return 1 ;;
    esac
    case "$OS" in
        Darwin) echo "$arch-apple-darwin" ;;
        Linux)
            if [ "$PM" = "apk" ]; then
                [ "$arch" = x86_64 ] && echo "x86_64-unknown-linux-musl" || return 1
            else
                # The built-in player needs ALSA at runtime.
                { ldconfig -p 2>/dev/null | grep -q 'libasound\.so\.2'; } || ls /usr/lib*/libasound.so.2 /usr/lib/*/libasound.so.2 >/dev/null 2>&1 || return 1
                echo "$arch-unknown-linux-gnu"
            fi ;;
        *) return 1 ;;
    esac
}
if [ "$CLONE" = 1 ] && [ "$FROM_SOURCE" = 0 ] && [ "$AUDIO" = 1 ] && command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
    if TRIPLE=$(release_target); then
        url="$RELEASES/latest/download/spotr-$TRIPLE.tar.xz"
        if curl -fsIL --max-time 8 -o /dev/null "$url" 2>/dev/null; then
            PREBUILT="$url"
            tag=$(curl -fsIL --max-time 8 -o /dev/null -w '%{url_effective}' "$RELEASES/latest" 2>/dev/null)
            tag=${tag##*/}; VERSION=${tag#v}
        fi
    fi
fi

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

# Step weights ≈ typical seconds, for the progress bar.
W_CLT=120; W_APT_UPDATE=10; W_PKGS=30; W_RUST=30; W_RUST_UPDATE=20; W_SRC=5; W_DOWNLOAD=8; W_INSTALL=1
if [ "$WARM" = 1 ]; then W_BUILD=15; else W_BUILD=120; fi

plan=()
if [ -n "$PREBUILT" ]; then
    # Nothing to compile, so no compiler, packages or Rust needed.
    NEED_CLT=0; MISSING=(); RUST_STATE="skip"
    plan+=("$ACC$ARROW$R Download Spotr${VERSION:+ $VERSION} $MUTED(prebuilt for $TRIPLE)$R")
    TOTAL_W=$((W_DOWNLOAD + W_INSTALL))
else
    TOTAL_W=$((W_BUILD + W_INSTALL))
    if [ "$NEED_CLT" = 1 ]; then
        plan+=("$ACC$ARROW$R Xcode Command Line Tools $MUTED(compiler, git)$R")
        TOTAL_W=$((TOTAL_W + W_CLT))
    fi
    if [ "${#MISSING[@]}" -gt 0 ]; then
        plan+=("$ACC$ARROW$R System packages $MUTED($PM)$R")
        plan+=("    $DIM${MISSING[*]}$R")
        TOTAL_W=$((TOTAL_W + W_PKGS))
        [ "$PM" = apt ] && TOTAL_W=$((TOTAL_W + W_APT_UPDATE))
    fi
    case "$RUST_STATE" in
        missing) plan+=("$ACC$ARROW$R Rust $MUTED(via rustup, into ~/.cargo and ~/.rustup)$R"); TOTAL_W=$((TOTAL_W + W_RUST)) ;;
        update)  plan+=("$ACC$ARROW$R Update Rust to stable $MUTED(found $have, need $MIN_RUST+)$R"); TOTAL_W=$((TOTAL_W + W_RUST_UPDATE)) ;;
    esac
    if [ "$CLONE" = 1 ]; then
        if [ -d "$SRC/.git" ]; then
            plan+=("$ACC$ARROW$R Update the source $MUTED(kept in ~${CACHE#"$HOME"})$R")
        else
            plan+=("$ACC$ARROW$R Download the source $MUTED(kept, so updates are quick)$R")
        fi
        TOTAL_W=$((TOTAL_W + W_SRC))
    fi
    verb="Build"; [ "$PREV_INSTALL" = 1 ] && verb="Rebuild"
    if [ "$WARM" = 1 ]; then
        plan+=("$ACC$ARROW$R $verb Spotr $MUTED(only what changed is compiled)$R")
    else
        plan+=("$ACC$ARROW$R $verb Spotr $MUTED(a few minutes the first time)$R")
    fi
    [ "$AUDIO" = 0 ] && plan+=("    ${DIM}without the built-in player (--no-audio)$R")
fi
plan+=("$ACC$ARROW$R Install it as $MUTED~/.cargo/bin/spotr$R")
already=()
[ "$PM" != "xcode" ] && [ "${#PKGS[@]}" -gt "${#MISSING[@]}" ] && already+=("system packages")
[ "$PM" = "xcode" ] && [ "$NEED_CLT" = 0 ] && already+=("Xcode tools")
[ "$RUST_STATE" = "ok" ] && already+=("Rust $have")

lines=("${plan[@]}")
if [ "${#already[@]}" -gt 0 ] && [ -z "$PREBUILT" ]; then
    joined=$(printf "%s, " "${already[@]}"); joined=${joined%, }
    lines+=("" "${MUTED}Already there: $joined$R")
fi
lines+=("" "${MUTED}Everything added is recorded, so ${R}./uninstall.sh${MUTED} can undo it.$R")
box "Here's the plan" "${lines[@]}"
echo

if [ "$RUST_STATE" = "old" ] && [ -z "$PREBUILT" ]; then
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
    step $W_CLT "Waiting for the Command Line Tools" wait_clt
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
        apt)    step $W_APT_UPDATE "Updating package lists" $SUDO apt-get update
                step $W_PKGS "Installing ${MISSING[*]}" $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y "${MISSING[@]}" ;;
        dnf)    step $W_PKGS "Installing ${MISSING[*]}" $SUDO dnf install -y "${MISSING[@]}" ;;
        pacman) step $W_PKGS "Installing ${MISSING[*]}" $SUDO pacman -S --needed --noconfirm "${MISSING[@]}" ;;
        zypper) step $W_PKGS "Installing ${MISSING[*]}" $SUDO zypper --non-interactive install "${MISSING[@]}" ;;
        apk)    step $W_PKGS "Installing ${MISSING[*]}" $SUDO apk add "${MISSING[@]}" ;;
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
        step $W_RUST "Installing Rust" install_rustup
        . "$HOME/.cargo/env" ;;
    update)
        step $W_RUST_UPDATE "Updating Rust" rustup update stable
        rustup default stable >>"$LOG" 2>&1 ;;
esac

# mkdir -p that records each directory it creates (deepest first), for uninstall.
make_dir() {
    local d="$1" made=()
    while [ ! -d "$d" ]; do made+=("$d"); d="$(dirname "$d")"; done
    mkdir -p "$1" || return 1
    for d in ${made[@]+"${made[@]}"}; do record "dir=$d"; done
}

BIN_DIR="$HOME/.cargo/bin"
make_dir "$BIN_DIR"
record "bin=$BIN_DIR/spotr"

if [ -n "$PREBUILT" ]; then
    DL="$(mktemp -d "${TMPDIR:-/tmp}/spotr-dl.XXXXXX")"
    fetch_release() {
        curl -fsSL "$PREBUILT" -o "$DL/spotr.tar.xz" || return 1
        # Check the archive against the checksum published with the release.
        if curl -fsSL "$PREBUILT.sha256" -o "$DL/sum" 2>/dev/null; then
            local want got
            want=$(awk '{print $1}' "$DL/sum")
            got=$( (shasum -a 256 "$DL/spotr.tar.xz" 2>/dev/null || sha256sum "$DL/spotr.tar.xz") | awk '{print $1}')
            [ "$want" = "$got" ] || { echo "checksum mismatch: expected $want, got $got"; return 1; }
        fi
        tar -xJf "$DL/spotr.tar.xz" -C "$DL"
    }
    step $W_DOWNLOAD "Downloading Spotr${VERSION:+ $VERSION}" fetch_release
    BUILT="$(find "$DL" -type f -name spotr -perm -u+x | head -n 1)"
    [ -n "$BUILT" ] || die "The release archive didn't contain a spotr binary." "Run again with $B--from-source$R to compile it instead."
    VERSION=$("$BUILT" --version 2>/dev/null | awk '{print $2}')
else
    if [ "$CLONE" = 1 ]; then
        make_dir "$CACHE" && record "cache=$CACHE"
        if [ -d "$SRC/.git" ]; then
            update_src() { git -C "$SRC" fetch --depth 1 origin "$BRANCH" && git -C "$SRC" reset --hard FETCH_HEAD; }
            step $W_SRC "Updating the source" update_src
        else
            step $W_SRC "Downloading the source" git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$SRC"
        fi
        VERSION=$(grep -m1 '^version' "$SRC/Cargo.toml" | cut -d'"' -f2)
    fi

    features=()
    [ "$AUDIO" = 0 ] && features=(--no-default-features)
    # `cargo build` into a kept target folder (instead of `cargo install`, which
    # starts from scratch every time), with the quick-to-rebuild `local` profile
    # (see Cargo.toml) and cargo's progress counter in the log.
    build() {
        cd "$SRC" && CARGO_TERM_PROGRESS_WHEN=always CARGO_TERM_PROGRESS_WIDTH=100 \
            cargo build --profile local --locked --target-dir "$TARGET_DIR" ${features[@]+"${features[@]}"}
    }
    step $W_BUILD "Building Spotr${VERSION:+ $VERSION}" build
    BUILT="$TARGET_DIR/local/spotr"
fi

# Copy to a temporary name first, so a running Spotr isn't disturbed mid-copy.
place() { cp "$BUILT" "$BIN_DIR/.spotr.new" && chmod 755 "$BIN_DIR/.spotr.new" && mv -f "$BIN_DIR/.spotr.new" "$BIN_DIR/spotr"; }
step $W_INSTALL "Installing to ~${BIN_DIR#"$HOME"}/spotr" place
end_steps

[ -n "$PREBUILT" ] && rm -rf "$DL"
rm -f "$LOG"

# ─────────────────────────────────────────────────────────────────── done

echo
final=("${GRN}${OK}${R} ${B}Spotr${VERSION:+ $VERSION} is installed.$R" "")
case ":$PATH:" in *":$BIN_DIR:"*) on_path=1 ;; *) on_path=0 ;; esac
if [ "$on_path" = 1 ] && [ "$RUST_STATE" != "missing" ]; then
    final+=("Start it with  ${KEY} spotr $R")
elif [ "$RUST_STATE" = "missing" ]; then
    # rustup added ~/.cargo/bin to the shell profiles.
    final+=("Open a new terminal, then start it with  ${KEY} spotr $R")
    final+=("${MUTED}(or right now: ${R}~/.cargo/bin/spotr${MUTED})$R")
else
    final+=("Start it with  ${KEY} ~/.cargo/bin/spotr $R")
    final+=("" "${MUTED}To type just ${R}spotr${MUTED}, add this to your shell's profile:$R")
    final+=("  export PATH=\"\$HOME/.cargo/bin:\$PATH\"")
fi
final+=("" "${MUTED}Just looking? Try ${R}spotr --demo${MUTED} — no account needed.$R")
final+=("${MUTED}Changed your mind? ${R}./uninstall.sh${MUTED} undoes all of this.$R")
box "All set" "${final[@]}"
echo
