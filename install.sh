#!/bin/sh
# Fast Text — universal installer.
#
#   curl -fsSL https://raw.githubusercontent.com/Fast-Text-editor/Fast-text-editor/main/install.sh | sh
#
# Installs the editor (`fast-text`, `ft`), the TextWrite language
# (`textwrite`, `tw`) and, with --gui, the desktop app. A prebuilt GitHub
# release is used when one exists for your platform; otherwise the script
# falls back to building from source with cargo.
#
# A POSIX shell is all that is needed to start. The prebuilt path also needs
# curl or wget plus tar; the source path needs cargo, and git unless the
# script is run from inside a checkout.

set -eu

REPO="Fast-Text-editor/Fast-text-editor"
PROGRAM="fast-text"

# Options (see usage()).
DRY_RUN=0
ASSUME_YES=0
FROM_SOURCE=0
WITH_GUI=0
DO_UNINSTALL=0
DO_CHECK=0
DO_UPDATE=0
NO_COMPLETIONS=0
VERSION="${FAST_TEXT_VERSION:-latest}"
PREFIX="${PREFIX:-}"
BIN_DIR="${BIN_DIR:-}"
SOURCE_DIR=""

# Filled in later.
SUDO=""
TMP_DIR=""
# Set when the install directory was taken from an existing install on PATH.
ADOPTED=0
# Windows shells (Git Bash, MSYS2, Cygwin) need the .exe suffix.
EXE_SUFFIX=""

BINARIES="fast-text ft textwrite tw"
GUI_BINARY="fast-text-gui"

# --------------------------------------------------------------------------
# Output and process helpers
# --------------------------------------------------------------------------

log() { printf '%s\n' "$*" >&2; }
warn() { printf '%s: warning: %s\n' "$PROGRAM" "$*" >&2; }
die() {
    printf '%s: error: %s\n' "$PROGRAM" "$*" >&2
    exit 1
}

# Runs a command, honouring --dry-run.
run() {
    if [ "$DRY_RUN" = 1 ]; then
        log "  would run: $*"
    else
        "$@"
    fi
}

# Runs a command with the privilege needed to write into the install tree.
run_priv() {
    if [ "$DRY_RUN" = 1 ]; then
        log "  would run:${SUDO:+ $SUDO} $*"
    elif [ -n "$SUDO" ]; then
        $SUDO "$@"
    else
        "$@"
    fi
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "'$1' is required but was not found on PATH"
}

usage() {
    cat <<EOF
$PROGRAM installer

Usage:
  install.sh [options]

Installs:
  fast-text, ft        the terminal editor
  textwrite, tw        the TextWrite language (also standalone)
  $GUI_BINARY     with --gui, the desktop app

Options:
  --prefix DIR        Install under DIR (default: \$HOME/.local, or
                      /usr/local when run as root)
  --bin-dir DIR       Put executables in DIR (overrides the prefix's bin)
  --version TAG       Install a specific release tag (default: latest).
                      With --from-source the tag is checked out instead.
  --from-source       Skip the prebuilt release and build with cargo
  --source DIR        Build from the checkout in DIR (implies --from-source)
  --gui               Also install the desktop app; source builds need the
                      system GUI development libraries
  --no-completions    Do not install shell completions
  --uninstall         Remove everything a previous run installed
  --check             Compare the installed version with the newest release
                      and print what to do next
  --update            Install the newest release over the current install
  -y, --yes           Do not prompt for confirmation
  --dry-run           Print what would happen and change nothing
  -h, --help          Show this help

Environment:
  PREFIX, BIN_DIR, FAST_TEXT_VERSION   behave like the options above
  FAST_TEXT_LATEST_TAG                 treat this as the newest release,
                                       skipping the GitHub query (for tests)

Re-running the installer over an existing installation updates that copy in
place (keeping the desktop app if it was installed before): --check reports
whether a newer release exists, and --update installs it only when there is
one.

Examples:
  curl -fsSL .../install.sh | sh
  sh install.sh --prefix "\$HOME/.local" -y
  sh install.sh --from-source --gui
EOF
}

# --------------------------------------------------------------------------
# Argument parsing
# --------------------------------------------------------------------------

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        --prefix)
            [ $# -ge 2 ] || die "--prefix needs a directory"
            PREFIX="$2"
            shift 2
            ;;
        --prefix=*)
            PREFIX="${1#--prefix=}"
            shift
            ;;
        --bin-dir)
            [ $# -ge 2 ] || die "--bin-dir needs a directory"
            BIN_DIR="$2"
            shift 2
            ;;
        --bin-dir=*)
            BIN_DIR="${1#--bin-dir=}"
            shift
            ;;
        --version)
            [ $# -ge 2 ] || die "--version needs a tag"
            VERSION="$2"
            shift 2
            ;;
        --version=*)
            VERSION="${1#--version=}"
            shift
            ;;
        --source)
            [ $# -ge 2 ] || die "--source needs a directory"
            SOURCE_DIR="$2"
            FROM_SOURCE=1
            shift 2
            ;;
        --from-source)
            FROM_SOURCE=1
            shift
            ;;
        --gui)
            WITH_GUI=1
            shift
            ;;
        --no-completions)
            NO_COMPLETIONS=1
            shift
            ;;
        --uninstall)
            DO_UNINSTALL=1
            shift
            ;;
        --check)
            DO_CHECK=1
            shift
            ;;
        --update)
            DO_UPDATE=1
            shift
            ;;
        -y | --yes)
            ASSUME_YES=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --)
            shift
            break
            ;;
        -*)
            die "unknown option '$1' (try --help)"
            ;;
        *)
            die "unexpected argument '$1' (try --help)"
            ;;
        esac
    done
    [ $# -eq 0 ] || die "unexpected argument '$1' (try --help)"
    : "$ASSUME_YES" # accepted for symmetry with other installers
}

# Expands a leading ~ and makes the path absolute.
abs_path() {
    case "$1" in
    "~") printf '%s\n' "$HOME" ;;
    "~/"*) printf '%s\n' "$HOME/${1#\~/}" ;;
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$(pwd)/$1" ;;
    esac
}

# When no location was named but Fast Text is already on PATH, aim an install
# at that copy so a plain run upgrades what the shell actually finds instead of
# quietly laying down a second copy somewhere else.
#
# `--uninstall` only follows the PATH copy when a manifest or version record
# proves it is one we installed — never a copy that cargo or a package manager
# put there.
adopt_existing_location() {
    if [ "$DO_CHECK" = 1 ]; then
        return 0
    fi
    if [ -n "$PREFIX" ] || [ -n "$BIN_DIR" ]; then
        return 0
    fi
    existing="$(command -v "$PROGRAM" 2>/dev/null || printf '')"
    if [ -z "$existing" ] || [ ! -f "$existing" ]; then
        return 0
    fi
    candidate_bin="$(abs_path "$(dirname "$existing")")"
    candidate_prefix="$(dirname "$candidate_bin")"
    if [ "$DO_UNINSTALL" = 1 ]; then
        if [ -f "$candidate_prefix/share/$PROGRAM/installed-files" ] ||
            [ -f "$candidate_prefix/share/$PROGRAM/installed-version" ]; then
            BIN_DIR="$candidate_bin"
            PREFIX="$candidate_prefix"
            ADOPTED=1
        fi
        return 0
    fi
    BIN_DIR="$candidate_bin"
    PREFIX="$candidate_prefix"
    ADOPTED=1
}

# Fills PREFIX and BIN_DIR, then decides whether sudo is needed.
resolve_dirs() {
    if [ -n "$BIN_DIR" ]; then
        BIN_DIR="$(abs_path "$BIN_DIR")"
        [ -n "$PREFIX" ] || PREFIX="$(dirname "$BIN_DIR")"
    else
        if [ -z "$PREFIX" ]; then
            if [ "$(id -u)" = 0 ]; then
                PREFIX="/usr/local"
            else
                PREFIX="$HOME/.local"
            fi
        fi
        PREFIX="$(abs_path "$PREFIX")"
        BIN_DIR="$PREFIX/bin"
    fi
    PREFIX="$(abs_path "$PREFIX")"

    SUDO=""
    # A version *check* only reads; it never needs elevated rights.
    if [ "$DO_CHECK" != 1 ] && [ "$(id -u)" != 0 ] && [ "$DRY_RUN" != 1 ]; then
        probe=""
        if [ -d "$BIN_DIR" ]; then
            probe="$BIN_DIR"
        elif [ -d "$PREFIX" ]; then
            probe="$PREFIX"
        else
            probe="$(dirname "$PREFIX")"
        fi
        if [ -d "$probe" ] && [ ! -w "$probe" ]; then
            SUDO="sudo"
        fi
    fi
    if [ "$DO_UNINSTALL" = 1 ]; then
        verb="Removing from"
    elif [ "$DO_CHECK" = 1 ]; then
        verb="Looking at"
    else
        verb="Installing into"
    fi
    if [ "$ADOPTED" = 1 ]; then
        log "Using the Fast Text already on your PATH: $BIN_DIR"
    fi
    if [ -n "$SUDO" ]; then
        command -v sudo >/dev/null 2>&1 ||
            die "$BIN_DIR is not writable and sudo is unavailable; pass --bin-dir DIR"
        log "$verb $BIN_DIR (with sudo)"
    else
        log "$verb $BIN_DIR"
    fi
}

# --------------------------------------------------------------------------
# Platform detection
# --------------------------------------------------------------------------

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
    Linux) os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        # Git Bash, MSYS2 and Cygwin can drive a source build, but the
        # prebuilt archives for Windows are ZIPs that install.ps1 unpacks.
        EXE_SUFFIX=".exe"
        if [ "$FROM_SOURCE" != 1 ]; then
            die "this looks like a Windows shell; run install.ps1 from PowerShell for the prebuilt build, or pass --from-source to build here"
        fi
        os_part="pc-windows-msvc"
        ;;
    *) die "unsupported operating system '$os'; try --from-source" ;;
    esac

    case "$arch" in
    x86_64 | amd64) arch_part="x86_64" ;;
    aarch64 | arm64) arch_part="aarch64" ;;
    armv7l | armv7) arch_part="armv7" ;;
    riscv64) arch_part="riscv64gc" ;;
    *) die "unsupported architecture '$arch'; try --from-source" ;;
    esac

    printf '%s-%s\n' "$arch_part" "$os_part"
}

# --------------------------------------------------------------------------
# Downloading a prebuilt release
# --------------------------------------------------------------------------

fetch() { # url dest
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --retry 2 "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        die "downloading a release needs curl or wget"
    fi
}

release_url() { # target
    # The release workflow names each archive `fast-text-<target>.tar.gz`
    # (no version in the name), so the same asset works through both the
    # rolling `latest` alias and a pinned `--version <tag>`.
    if [ "$VERSION" = "latest" ]; then
        base="releases/latest/download"
    else
        base="releases/download/$VERSION"
    fi
    printf 'https://github.com/%s/%s/%s-%s.tar.gz\n' \
        "$REPO" "$base" "$PROGRAM" "$1"
}

# Downloads and unpacks a release into $TMP_DIR/release; returns non-zero
# (without dying) when no suitable release exists.
download_release() { # target
    url="$(release_url "$1")"
    archive="$TMP_DIR/$PROGRAM.tar.gz"
    log "Downloading $url"
    mkdir -p "$TMP_DIR/release"
    fetch "$url" "$archive" 2>/dev/null || {
        warn "no prebuilt release was found for $1"
        return 1
    }
    tar -xzf "$archive" -C "$TMP_DIR/release" 2>/dev/null || {
        warn "the release archive could not be unpacked"
        return 1
    }
    return 0
}

# --------------------------------------------------------------------------
# Building from source
# --------------------------------------------------------------------------

build_from_source() {
    src="$SOURCE_DIR"
    if [ -z "$src" ]; then
        if [ -f "Cargo.toml" ] && [ -d "crates/fast-text-tui" ]; then
            src="$(pwd)"
            log "Building from the current checkout"
        else
            need git
            src="$TMP_DIR/src"
            log "Cloning https://github.com/$REPO.git"
            if [ "$VERSION" = "latest" ]; then
                run git clone --depth 1 "https://github.com/$REPO.git" "$src"
            else
                run git clone --depth 1 --branch "$VERSION" "https://github.com/$REPO.git" "$src"
            fi
        fi
    fi
    [ -d "$src" ] || die "source directory '$src' does not exist"
    [ -f "$src/Cargo.toml" ] || die "'$src' is not a Fast Text checkout"
    need cargo

    set -- -p fast-text-tui -p textwrite
    if [ "$WITH_GUI" = 1 ]; then
        set -- "$@" -p fast-text-gui
    fi
    log "cargo build --release $*"
    if [ "$DRY_RUN" = 1 ]; then
        run cargo build --release "$@"
        return 0
    fi
    (cd "$src" && cargo build --release "$@") || die "the build failed"

    mkdir -p "$TMP_DIR/release"
    for name in $BINARIES $GUI_BINARY; do
        for candidate in "$name$EXE_SUFFIX" "$name"; do
            if [ -f "$src/target/release/$candidate" ]; then
                cp "$src/target/release/$candidate" "$TMP_DIR/release/$name"
                break
            fi
        done
    done
}

# --------------------------------------------------------------------------
# Installing files
# --------------------------------------------------------------------------

find_binary() { # name dir
    found="$(find "$2" -type f \( -name "$1" -o -name "$1.exe" \) 2>/dev/null | head -n 1)"
    printf '%s\n' "$found"
}

ensure_dir() { # dir
    [ -d "$1" ] && return 0
    run_priv mkdir -p "$1"
}

copy_file() { # src dest
    run_priv install -m 0755 "$1" "$2" 2>/dev/null && return 0
    run_priv cp "$1" "$2" || return 1
    run_priv chmod 0755 "$2"
}

# Writes stdin to a file; used for the small completion scripts.
write_file() { # dest
    if [ "$DRY_RUN" = 1 ]; then
        log "  would write $1"
        cat >/dev/null
        return 0
    fi
    if [ -n "$SUDO" ]; then
        $SUDO tee "$1" >/dev/null
    else
        cat >"$1"
    fi
}

install_completions() {
    bash_dir="$PREFIX/share/bash-completion/completions"
    zsh_dir="$PREFIX/share/zsh/site-functions"
    ensure_dir "$bash_dir"
    ensure_dir "$zsh_dir"

    for name in fast-text ft; do
        write_file "$bash_dir/$name" <<'EOF'
_fast_text() {
    local cur prev
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    case "$prev" in
        -c|--config) COMPREPLY=( $(compgen -f -- "$cur") ); return 0 ;;
    esac
    case "$cur" in
        -*) COMPREPLY=( $(compgen -W "-c --config -h --help -V --version" -- "$cur") ) ;;
        *) COMPREPLY=( $(compgen -f -- "$cur") ) ;;
    esac
}
complete -o default -F _fast_text fast-text ft
EOF
    done

    for name in textwrite tw; do
        write_file "$bash_dir/$name" <<'EOF'
_textwrite() {
    local cur
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    case "$cur" in
        -*) COMPREPLY=( $(compgen -W "-h --help -V --version" -- "$cur") ) ;;
        *) COMPREPLY=( $(compgen -f -X '!*.tw' -- "$cur"; compgen -d -- "$cur") ) ;;
    esac
}
complete -o default -F _textwrite textwrite tw
EOF
    done

    write_file "$zsh_dir/_fast-text" <<'EOF'
#compdef fast-text ft
_arguments \
    '-c[read settings from a config file]:config file:_files' \
    '--config[read settings from a config file]:config file:_files' \
    '-h[print help]' '--help[print help]' \
    '-V[print version]' '--version[print version]' \
    '*:file:_files'
EOF

    write_file "$zsh_dir/_textwrite" <<'EOF'
#compdef textwrite tw
_arguments \
    '-h[print help]' '--help[print help]' \
    '-V[print version]' '--version[print version]' \
    '*:script:_files -g "*.tw"'
EOF

    for dest in \
        "$bash_dir/fast-text" "$bash_dir/ft" \
        "$bash_dir/textwrite" "$bash_dir/tw" \
        "$zsh_dir/_fast-text" "$zsh_dir/_textwrite"; do
        printf '%s\n' "$dest" >>"$TMP_DIR/manifest"
    done
}

save_manifest() {
    manifest="$PREFIX/share/$PROGRAM/installed-files"
    if [ "$DRY_RUN" = 1 ]; then
        log "  would write $manifest"
        return 0
    fi
    run_priv mkdir -p "$(dirname "$manifest")"
    run_priv cp "$TMP_DIR/manifest" "$manifest"
}

install_binaries() {
    : >"$TMP_DIR/manifest"
    ensure_dir "$BIN_DIR"

    if [ "$DRY_RUN" = 1 ]; then
        if [ "$WITH_GUI" = 1 ]; then
            log "Would install into $BIN_DIR: $BINARIES $GUI_BINARY"
        else
            log "Would install into $BIN_DIR: $BINARIES"
        fi
        return 0
    fi

    log "Installing executables into $BIN_DIR"

    installed=""
    for name in $BINARIES; do
        src_file="$(find_binary "$name" "$TMP_DIR/release")"
        if [ -z "$src_file" ]; then
            warn "'$name' was not produced; skipping"
            continue
        fi
        copy_file "$src_file" "$BIN_DIR/$name$EXE_SUFFIX"
        printf '%s\n' "$BIN_DIR/$name$EXE_SUFFIX" >>"$TMP_DIR/manifest"
        installed="$installed $name"
    done

    if [ "$WITH_GUI" = 1 ]; then
        src_file="$(find_binary "$GUI_BINARY" "$TMP_DIR/release")"
        if [ -z "$src_file" ]; then
            warn "'$GUI_BINARY' was not produced; skipping"
        else
            copy_file "$src_file" "$BIN_DIR/$GUI_BINARY$EXE_SUFFIX"
            printf '%s\n' "$BIN_DIR/$GUI_BINARY$EXE_SUFFIX" >>"$TMP_DIR/manifest"
            installed="$installed $GUI_BINARY"
        fi
    fi

    [ -n "$installed" ] || die "no executables were installed"
    log "Installed:$installed"
}

# --------------------------------------------------------------------------
# Uninstall
# --------------------------------------------------------------------------

uninstall() {
    manifest="$PREFIX/share/$PROGRAM/installed-files"
    log "Uninstalling from $PREFIX"
    if [ -f "$manifest" ]; then
        while IFS= read -r file; do
            [ -n "$file" ] || continue
            if [ -e "$file" ]; then
                run_priv rm -f "$file"
                log "  removed $file"
            fi
        done <"$manifest"
        run_priv rm -f "$manifest"
    else
        warn "no manifest at $manifest; removing the known binaries instead"
        for name in $BINARIES $GUI_BINARY; do
            if [ -e "$BIN_DIR/$name$EXE_SUFFIX" ]; then
                run_priv rm -f "$BIN_DIR/$name$EXE_SUFFIX"
                log "  removed $BIN_DIR/$name$EXE_SUFFIX"
            fi
        done
        if [ -e "$(version_file)" ]; then
            run_priv rm -f "$(version_file)"
            log "  removed $(version_file)"
        fi
    fi
    log "Done. Your settings and plugins under ~/.config/fast-text were left alone."
}

# --------------------------------------------------------------------------
# Version checks and updates
# --------------------------------------------------------------------------

# Where the installed version is recorded. It is listed in the manifest, so
# `--uninstall` removes it along with the executables.
version_file() {
    printf '%s/share/%s/installed-version\n' "$PREFIX" "$PROGRAM"
}

# The version a previous install recorded, if there is a record.
read_installed_version() {
    file="$(version_file)"
    [ -f "$file" ] || return 1
    head -n 1 "$file"
}

# True when version $1 is strictly newer than version $2. Plain numeric
# comparison, so it works without GNU `sort -V` (absent on macOS and the BSDs).
version_gt() {
    a="${1#v}"
    b="${2#v}"
    while :; do
        an="${a%%.*}"
        bn="${b%%.*}"
        an="$(printf '%s' "$an" | tr -cd '0-9')"
        bn="$(printf '%s' "$bn" | tr -cd '0-9')"
        if [ -z "$an" ]; then an=0; fi
        if [ -z "$bn" ]; then bn=0; fi
        if [ "$an" -gt "$bn" ]; then return 0; fi
        if [ "$an" -lt "$bn" ]; then return 1; fi
        case "$a" in
        *.*) a="${a#*.}" ;;
        *) a="" ;;
        esac
        case "$b" in
        *.*) b="${b#*.}" ;;
        *) b="" ;;
        esac
        if [ -z "$a" ] && [ -z "$b" ]; then
            return 1
        fi
        if [ -z "$a" ]; then a=0; fi
        if [ -z "$b" ]; then b=0; fi
    done
}

# The newest release tag, straight from the GitHub API. Setting
# FAST_TEXT_LATEST_TAG skips the network query — the tests rely on that.
latest_tag() {
    if [ -n "${FAST_TEXT_LATEST_TAG:-}" ]; then
        printf '%s\n' "$FAST_TEXT_LATEST_TAG"
        return 0
    fi
    json=""
    if command -v curl >/dev/null 2>&1; then
        json="$(curl -fsSL --max-time 20 \
            "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null)" || return 1
    elif command -v wget >/dev/null 2>&1; then
        json="$(wget -q -O - --timeout=20 \
            "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null)" || return 1
    else
        return 1
    fi
    tag="$(printf '%s' "$json" |
        sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
        head -n 1)"
    [ -n "$tag" ] || return 1
    printf '%s\n' "$tag"
}

# Records which version was installed, so later `--check`/`--update` runs and
# the editor's own Alt+U check have something to compare against.
write_version_file() {
    file="$(version_file)"
    version=""
    exe="$BIN_DIR/$PROGRAM$EXE_SUFFIX"
    if [ "$DRY_RUN" != 1 ] && [ -x "$exe" ]; then
        # Ask the freshly installed editor itself — the most reliable source.
        version="$("$exe" --version 2>/dev/null |
            sed -n 's/^[^ ]*[[:space:]]*//p' | head -n 1)" || version=""
    fi
    if [ -z "$version" ]; then
        version="$(newest_tag 2>/dev/null || printf '')"
    fi
    version="${version#v}"
    if [ -z "$version" ]; then
        warn "could not work out which version was installed"
        return 0
    fi
    if [ "$DRY_RUN" = 1 ]; then
        log "  would record version $version in $file"
        return 0
    fi
    printf '%s\n' "$version" >"$TMP_DIR/version"
    run_priv mkdir -p "$(dirname "$file")"
    run_priv cp "$TMP_DIR/version" "$file"
    printf '%s\n' "$file" >>"$TMP_DIR/manifest"
}

# Prints the installed version and the newest release, plus what to do next.
check_update() {
    installed="$(read_installed_version)" || installed=""
    on_path=""
    if [ -z "$installed" ]; then
        # No record here, but Fast Text may have been installed another way
        # (cargo install, a package manager, a differently-prefixed run).
        exe="$(command -v "$PROGRAM" 2>/dev/null || printf '')"
        if [ -n "$exe" ]; then
            installed="$("$exe" --version 2>/dev/null |
                sed -n 's/^[^ ]*[[:space:]]*//p' | head -n 1)" || installed=""
            if [ -n "$installed" ]; then
                log "Found $PROGRAM at $exe"
                on_path="$exe"
            fi
        fi
    fi
    if ! newest="$(newest_tag)"; then
        die "could not reach GitHub to check for a newer release"
    fi
    if [ -z "$installed" ]; then
        log "No Fast Text installation was found under $PREFIX (or on PATH)."
        log "The newest release is v$newest."
        log "Install it with: sh install.sh --version v$newest --prefix $PREFIX"
        return 0
    fi
    installed="${installed#v}"
    if version_gt "$newest" "$installed"; then
        log "Update available: v$installed is installed, v$newest has been released."
        if [ -n "$on_path" ]; then
            log "Install it with: sh install.sh --update"
        else
            log "Install it with: sh install.sh --update --prefix $PREFIX"
        fi
    else
        log "Fast Text v$installed is up to date (newest release: v$newest)."
    fi
}

# Keeps `--gui` when an earlier run installed the desktop app, so an update
# does not silently drop it.
adopt_gui_from_manifest() {
    if [ "$WITH_GUI" = 1 ]; then
        return 0
    fi
    manifest="$PREFIX/share/$PROGRAM/installed-files"
    if [ -f "$manifest" ] && grep -q "/$GUI_BINARY" "$manifest" 2>/dev/null; then
        WITH_GUI=1
        log "The desktop app was installed before; it will be updated as well"
    fi
}

# The newest release tag, or an empty string when GitHub cannot be reached.
newest_tag() {
    if ! tag="$(latest_tag)"; then
        return 1
    fi
    printf '%s\n' "${tag#v}"
}

# A plain run over an existing installation is an upgrade: reuse its version
# record, keep the desktop app if it was installed, and install the newest
# release (or the one named by --version). This is what makes simply re-running
# the installer deliver new features to the copy that is already there.
adopt_existing_install() {
    installed="$(read_installed_version)" || installed=""
    if [ -z "$installed" ] && [ ! -e "$BIN_DIR/$PROGRAM$EXE_SUFFIX" ]; then
        return 0
    fi
    if [ -n "$installed" ]; then
        log "Fast Text v${installed#v} is already installed under $PREFIX; updating it"
    else
        log "Found an existing $PROGRAM in $BIN_DIR; reinstalling over it"
    fi
    adopt_gui_from_manifest

    if [ "$VERSION" != "latest" ]; then
        log "Installing $VERSION"
        return 0
    fi
    if ! newest="$(newest_tag)"; then
        log "Could not reach GitHub; installing the 'latest' release instead"
        return 0
    fi
    VERSION="v$newest"
    if [ -n "$installed" ]; then
        log "Updating from v${installed#v} to v$newest"
    else
        log "Installing v$newest"
    fi
}

# Points VERSION at the newest release when an update is actually warranted.
# Used by `--update`, which leaves an up-to-date install alone.
update_now() {
    if [ "$VERSION" != "latest" ]; then
        log "Updating to $VERSION"
        adopt_gui_from_manifest
        return 0
    fi
    installed="$(read_installed_version)" || installed=""
    if ! newest="$(newest_tag)"; then
        die "could not reach GitHub to find the newest release"
    fi
    if [ -n "$installed" ] && ! version_gt "$newest" "${installed#v}"; then
        log "Fast Text v${installed#v} is already up to date (newest release: v$newest)."
        exit 0
    fi
    if [ -n "$installed" ]; then
        log "Updating from v${installed#v} to v$newest"
    else
        log "Installing the newest release, v$newest"
    fi
    VERSION="v$newest"
    adopt_gui_from_manifest
}

# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

main() {
    parse_args "$@"
    adopt_existing_location
    resolve_dirs

    if [ "$DO_UNINSTALL" = 1 ]; then
        uninstall
        exit 0
    fi

    if [ "$DO_CHECK" = 1 ]; then
        check_update
        exit 0
    fi

    if [ "$DO_UPDATE" = 1 ]; then
        update_now
    else
        adopt_existing_install
    fi

    TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/fast-text-install.XXXXXX")" ||
        die "could not create a temporary directory"
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT INT TERM

    if [ "$DRY_RUN" = 1 ]; then
        log "Dry run: nothing will be changed."
    fi

    target="$(detect_target)"
    if [ "$FROM_SOURCE" = 1 ]; then
        log "Building from source for $target"
        build_from_source
    elif [ "$DRY_RUN" = 1 ]; then
        log "Would try the prebuilt release at $(release_url "$target")"
        log "Falling back to a source build when it is missing"
        build_from_source
    else
        log "Looking for a prebuilt release for $target"
        if ! download_release "$target"; then
            log "Falling back to a source build"
            build_from_source
        fi
    fi

    install_binaries
    write_version_file
    if [ "$NO_COMPLETIONS" != 1 ]; then
        install_completions
    fi
    save_manifest

    log "Done."
    if [ "$NO_COMPLETIONS" != 1 ]; then
        log "Shell completions installed under $PREFIX/share."
    fi
    log "Your config lives at \${XDG_CONFIG_HOME:-\$HOME/.config}/fast-text/config.toml"
    log "Uninstall later with: sh install.sh --uninstall${PREFIX:+ --prefix $PREFIX}"
}

main "$@"
