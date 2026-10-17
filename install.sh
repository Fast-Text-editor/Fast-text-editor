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
NO_COMPLETIONS=0
VERSION="${FAST_TEXT_VERSION:-latest}"
PREFIX="${PREFIX:-}"
BIN_DIR="${BIN_DIR:-}"
SOURCE_DIR=""

# Filled in later.
SUDO=""
TMP_DIR=""

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
  -y, --yes           Do not prompt for confirmation
  --dry-run           Print what would happen and change nothing
  -h, --help          Show this help

Environment:
  PREFIX, BIN_DIR, FAST_TEXT_VERSION   behave like the options above

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
    if [ "$(id -u)" != 0 ] && [ "$DRY_RUN" != 1 ]; then
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
    else
        verb="Installing into"
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
        if [ -f "$src/target/release/$name" ]; then
            cp "$src/target/release/$name" "$TMP_DIR/release/$name"
        fi
    done
}

# --------------------------------------------------------------------------
# Installing files
# --------------------------------------------------------------------------

find_binary() { # name dir
    found="$(find "$2" -type f -name "$1" 2>/dev/null | head -n 1)"
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
        copy_file "$src_file" "$BIN_DIR/$name"
        printf '%s\n' "$BIN_DIR/$name" >>"$TMP_DIR/manifest"
        installed="$installed $name"
    done

    if [ "$WITH_GUI" = 1 ]; then
        src_file="$(find_binary "$GUI_BINARY" "$TMP_DIR/release")"
        if [ -z "$src_file" ]; then
            warn "'$GUI_BINARY' was not produced; skipping"
        else
            copy_file "$src_file" "$BIN_DIR/$GUI_BINARY"
            printf '%s\n' "$BIN_DIR/$GUI_BINARY" >>"$TMP_DIR/manifest"
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
            if [ -e "$BIN_DIR/$name" ]; then
                run_priv rm -f "$BIN_DIR/$name"
                log "  removed $BIN_DIR/$name"
            fi
        done
    fi
    log "Done. Your settings and plugins under ~/.config/fast-text were left alone."
}

# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

main() {
    parse_args "$@"
    resolve_dirs

    if [ "$DO_UNINSTALL" = 1 ]; then
        uninstall
        exit 0
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
