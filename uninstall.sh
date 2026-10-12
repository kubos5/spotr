#!/usr/bin/env bash
# Spotr uninstaller: undoes what install.sh did, using the manifest it wrote.
#
#   ./uninstall.sh
#   curl -fsSL https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.sh | bash
#
# Only what the installer added is removed: packages and Rust that were already
# on the system stay. Your settings are kept unless you ask for them to go.
#
# Copyright (C) Spotr contributors. Licensed under the GNU GPL v3 or later.

set -u

DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/spotr"
MANIFEST="$DATA_DIR/install-manifest"
LOG="$(mktemp "${TMPDIR:-/tmp}/spotr-uninstall.XXXXXX")"

ASSUME_YES=0
KEEP_DEPS=0
PURGE=""
for arg in "$@"; do
    case "$arg" in
        -y|--yes) ASSUME_YES=1 ;;
        --keep-deps) KEEP_DEPS=1 ;;
        --purge) PURGE=1 ;;
        --keep-settings) PURGE=0 ;;
        -h|--help)
            cat <<'EOF'
Spotr uninstaller

USAGE:
    ./uninstall.sh [OPTIONS]

OPTIONS:
    -y, --yes           Don't ask for confirmation (keeps your settings unless --purge)
    --keep-deps         Only remove Spotr; keep the packages and Rust the installer added
    --purge             Also delete Spotr's settings, saved login and cache
    --keep-settings     Keep Spotr's settings, saved login and cache
    -h, --help          Show this help
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

# ───────────────────────────────────────────────────────────── what to undo

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

value() { grep -m1 "^$1=" "$MANIFEST" 2>/dev/null | cut -d= -f2-; }

BIN="$HOME/.cargo/bin/spotr"
PM=""; PKGS=(); RUSTUP=0; CLT=0; BUILD_CACHE=""
if [ -f "$MANIFEST" ]; then
    BIN="$(value bin)"; BIN="${BIN:-$HOME/.cargo/bin/spotr}"
    PM="$(value pm)"
    while IFS= read -r p; do PKGS+=("$p"); done < <(grep '^pkg=' "$MANIFEST" | cut -d= -f2-)
    [ "$(value rustup)" = 1 ] && RUSTUP=1
    [ "$(value xcode_clt)" = 1 ] && CLT=1
    BUILD_CACHE="$(value cache)"
fi
if [ "$KEEP_DEPS" = 1 ]; then PKGS=(); RUSTUP=0; CLT=0; fi

# Spotr knows where its own files live; ask it before it's gone.
CONFIG_DIR=""; CACHE_DIR=""
if [ -x "$BIN" ]; then
    paths="$("$BIN" --print-paths 2>/dev/null)"
    cfg_file="$(printf '%s\n' "$paths" | sed -n 's/^config  *//p')"
    [ -n "$cfg_file" ] && CONFIG_DIR="$(dirname "$cfg_file")"
    CACHE_DIR="$(printf '%s\n' "$paths" | sed -n 's/^cache  *//p')"
fi

banner "uninstaller"

if [ ! -f "$MANIFEST" ] && [ ! -e "$BIN" ]; then
    box "Nothing to do" "Spotr doesn't seem to be installed." "${MUTED}No install manifest, and no ${R}$BIN$MUTED.$R"
    echo
    exit 0
fi

plan=()
short() { case "$1" in "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;; *) printf '%s' "$1" ;; esac; }
[ -e "$BIN" ] && plan+=("$RED$FAIL$R Spotr $MUTED($(short "$BIN"))$R")
if [ "${#PKGS[@]}" -gt 0 ]; then
    plan+=("$RED$FAIL$R System packages the installer added $MUTED($PM)$R")
    plan+=("    $DIM${PKGS[*]}$R")
fi
[ -n "$BUILD_CACHE" ] && [ -d "$BUILD_CACHE" ] && plan+=("$RED$FAIL$R Spotr's source and build cache $MUTED($(short "$BUILD_CACHE"))$R")
[ "$RUSTUP" = 1 ] && plan+=("$RED$FAIL$R Rust $MUTED(rustup, ~/.cargo, ~/.rustup and its PATH setup)$R")
[ "$CLT" = 1 ] && plan+=("$RED$FAIL$R Xcode Command Line Tools $MUTED(/Library/Developer/CommandLineTools)$R")
[ -f "$MANIFEST" ] && plan+=("$RED$FAIL$R The install manifest $MUTED(~${MANIFEST#"$HOME"})$R")
if [ ! -f "$MANIFEST" ]; then
    plan+=("" "${YEL}!$R ${MUTED}No install manifest, so only Spotr itself is removed.$R")
elif [ "$KEEP_DEPS" = 1 ]; then
    plan+=("" "${MUTED}Keeping the packages and Rust the installer added (--keep-deps).$R")
fi
if { [ -d "$CONFIG_DIR" ] || [ -d "$CACHE_DIR" ]; } && [ -z "$PURGE" ]; then
    plan+=("" "${MUTED}You'll be asked separately about your settings and saved login.$R")
fi
box "This will remove" "${plan[@]}"
echo

ask "Uninstall now? ${MUTED}[${R}Y${MUTED}/n]$R" y || { printf '\n  %sNothing was changed.%s\n\n' "$MUTED" "$R"; exit 0; }

if [ -z "$PURGE" ] && { [ -d "$CONFIG_DIR" ] || [ -d "$CACHE_DIR" ]; }; then
    if [ "$ASSUME_YES" = 1 ]; then
        PURGE=0
    else
        echo
        where=()
        [ -d "$CONFIG_DIR" ] && where+=("${MUTED}settings & login  ${R}$(short "$CONFIG_DIR")")
        [ -d "$CACHE_DIR" ] && where+=("${MUTED}cache & logs      ${R}$(short "$CACHE_DIR")")
        box "Your Spotr data" "${where[@]}"
        echo
        if ask "Delete it too? ${MUTED}[y/${R}N${MUTED}]$R" n; then PURGE=1; else PURGE=0; fi
    fi
fi
echo

SUDO=""
[ "$(id -u)" != 0 ] && SUDO="sudo"
SUDO_READY=0
need_sudo() {
    [ -z "$SUDO" ] || [ "$SUDO_READY" = 1 ] && return 0
    command -v sudo >/dev/null 2>&1 || die "Removing system files needs root, and sudo isn't available."
    info_line "$1"
    sudo -v </dev/tty || die "sudo failed, so the rest was kept." "Run ./uninstall.sh again to retry."
    ( while kill -0 $$ 2>/dev/null; do sudo -n true 2>/dev/null; sleep 50; done ) &
    SUDO_READY=1
}

# ──────────────────────────────────────────────────────────────── uninstall

if [ -e "$BIN" ]; then
    # Older installers used `cargo install`; let cargo forget it too, but only if
    # cargo's record is about this very file (CARGO_HOME may point elsewhere).
    if command -v cargo >/dev/null 2>&1 && [ "${CARGO_HOME:-$HOME/.cargo}/bin/spotr" = "$BIN" ] \
        && cargo install --list 2>/dev/null | grep -q '^spotr '; then
        remove_bin() { cargo uninstall spotr; rm -f "$BIN"; }
    else
        remove_bin() { rm -f "$BIN"; }
    fi
    step 1 "Removing Spotr" remove_bin
fi

if [ -n "$BUILD_CACHE" ] && [ -d "$BUILD_CACHE" ]; then
    step 1 "Removing the build cache" rm -rf "$BUILD_CACHE"
fi

if [ "$PURGE" = 1 ]; then
    rm_data() { rm -rf ${CONFIG_DIR:+"$CONFIG_DIR"} ${CACHE_DIR:+"$CACHE_DIR"}; }
    step 1 "Deleting your Spotr settings and cache" rm_data
fi

if [ "$RUSTUP" = 1 ] && command -v rustup >/dev/null 2>&1; then
    step 1 "Removing Rust" rustup self uninstall -y
fi

if [ "${#PKGS[@]}" -gt 0 ]; then
    # Only the ones still installed (you may have removed some yourself).
    present=()
    for p in "${PKGS[@]}"; do
        case "$PM" in
            apt) dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "ok installed" && present+=("$p") ;;
            dnf|zypper) rpm -q "$p" >/dev/null 2>&1 && present+=("$p") ;;
            pacman) pacman -Qi "$p" >/dev/null 2>&1 && present+=("$p") ;;
            apk) apk info -e "$p" >/dev/null 2>&1 && present+=("$p") ;;
        esac
    done
    if [ "${#present[@]}" -gt 0 ]; then
        need_sudo "Removing system packages needs your password."
        case "$PM" in
            apt)    step 1 "Removing ${present[*]}" $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y "${present[@]}"
                    step 1 "Removing packages nothing needs anymore" $SUDO env DEBIAN_FRONTEND=noninteractive apt-get autoremove -y ;;
            dnf)    step 1 "Removing ${present[*]}" $SUDO dnf remove -y "${present[@]}" ;;
            pacman) step 1 "Removing ${present[*]}" $SUDO pacman -Rns --noconfirm "${present[@]}" ;;
            zypper) step 1 "Removing ${present[*]}" $SUDO zypper --non-interactive remove --clean-deps "${present[@]}" ;;
            apk)    step 1 "Removing ${present[*]}" $SUDO apk del "${present[@]}" ;;
        esac
    fi
fi

if [ "$CLT" = 1 ] && [ -d /Library/Developer/CommandLineTools ]; then
    need_sudo "Removing the Command Line Tools needs your password."
    step 1 "Removing the Xcode Command Line Tools" $SUDO rm -rf /Library/Developer/CommandLineTools
fi

if [ -f "$MANIFEST" ]; then
    remove_manifest() {
        local f d
        # Files rustup created that are now empty, and directories we created that are empty.
        if [ "$RUSTUP" = 1 ]; then
            grep '^file=' "$MANIFEST" | cut -d= -f2- | while IFS= read -r f; do
                [ -f "$f" ] && [ ! -s "$f" ] && rm -f "$f"
            done
        fi
        dirs=$(grep '^dir=' "$MANIFEST" | cut -d= -f2-)
        rm -f "$MANIFEST"
        rmdir "$DATA_DIR" 2>/dev/null
        printf '%s\n' "$dirs" | while IFS= read -r d; do
            [ -n "$d" ] && rmdir "$d" 2>/dev/null
        done
        return 0
    }
    step 1 "Cleaning up" remove_manifest
fi
rm -f "$LOG"

# ─────────────────────────────────────────────────────────────────── done

echo
final=("${GRN}${OK}${R} ${B}Spotr has been uninstalled.$R")
if [ "$PURGE" != 1 ] && { [ -d "$CONFIG_DIR" ] || [ -d "$CACHE_DIR" ]; }; then
    final+=("" "${MUTED}Your settings were kept, in case you come back:$R")
    [ -d "$CONFIG_DIR" ] && final+=("  $(short "$CONFIG_DIR")")
    [ -d "$CACHE_DIR" ] && final+=("  $(short "$CACHE_DIR")")
fi
[ "$RUSTUP" = 1 ] && final+=("" "${MUTED}Open a new terminal so the old PATH is gone.$R")
if [ "$UNI" = 1 ]; then
    final+=("" "${MUTED}Thanks for trying Spotr ${ACC}♪$R")
else
    final+=("" "${MUTED}Thanks for trying Spotr.$R")
fi
box "Goodbye" "${final[@]}"
echo
