#!/bin/sh
# probbit installer: fetches a prebuilt release archive, checks its SHA-256 and installs the `probbit` binary. Never uses sudo.
#
#   curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/BitmapAsset/probbit/main/install.sh | PROBBIT_VERSION=v0.14.0 sh
#
# Environment (all optional):
#   PROBBIT_VERSION        release tag, e.g. v0.14.0 (default: the latest release)
#   PROBBIT_INSTALL_DIR    where `probbit` goes (default: ~/.local/bin; never overwrites a system-wide install implicitly)
#   PROBBIT_DOWNLOAD_BASE  the archive is fetched from $PROBBIT_DOWNLOAD_BASE/<tag>/probbit-<tag>-<target>.tar.gz
#                       (default: https://github.com/BitmapAsset/probbit/releases/download); needs PROBBIT_VERSION
#   PROBBIT_TARGET         Rust target triple to fetch instead of the detected one
#   PROBBIT_MAN_DIR        where the manual page probbit.1 goes, when the archive has one (default: <prefix>/share/man/man1
#                       when PROBBIT_INSTALL_DIR is <prefix>/bin, else ~/.local/share/man/man1, and the installer says so
#                       when that is outside PROBBIT_INSTALL_DIR; a failure there is reported and installs nothing less)
# Needs curl or wget, tar, and one of sha256sum, shasum or openssl.
set -eu

REPO="BitmapAsset/probbit"

say() { printf '%s\n' "$*"; }
die() { printf 'probbit install: error: %s\n' "$*" >&2; exit 1; }
has() { command -v "$1" > /dev/null 2>&1; }

fetch() { # fetch URL FILE
    if has curl; then
        case "$1" in
            https://*) curl --proto '=https' --proto-redir '=https' -fsSL "$1" -o "$2" ;;
            *) curl -fsSL "$1" -o "$2" ;;
        esac
    elif has wget; then
        wget -q -O "$2" "$1"
    else
        die "need curl or wget"
    fi
}

sha256() {
    if has sha256sum; then sha256sum "$1" | cut -d ' ' -f 1
    elif has shasum; then shasum -a 256 "$1" | cut -d ' ' -f 1
    elif has openssl; then openssl dgst -sha256 "$1" | sed 's/^.*= *//'
    else die "need sha256sum, shasum or openssl to check the download"
    fi
}

# The x86_64 gnu build needs glibc >= 2.34 (release.yml): an older glibc (Debian 11, Ubuntu 20.04, RHEL 8) gets the static musl
# build. The version is the last number on the first line of `ldd --version`; none found keeps the gnu build.
glibc_before_2_34() {
    v=$( (ldd --version 2>&1 || true) | head -n 1 | sed -n 's/.*[^0-9.]\([0-9][0-9]*\)\.\([0-9][0-9]*\)[^ ]*$/\1 \2/p')
    [ -n "$v" ] || return 1
    set -- $v
    [ "$1" -lt 2 ] || { [ "$1" -eq 2 ] && [ "$2" -lt 34 ]; }
}
detect_target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "$os" in
        Darwin)
            # a shell running under Rosetta reports x86_64 on Apple silicon: take the native build
            if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2> /dev/null || echo 0)" = 1 ]; then arch=arm64; fi
            case "$arch" in
                arm64 | aarch64) echo aarch64-apple-darwin ;;
                x86_64) echo x86_64-apple-darwin ;;
                *) die "no prebuilt probbit for macOS on $arch" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64 | amd64)
                    if (ldd --version 2>&1 || true) | grep -qi musl; then
                        echo x86_64-unknown-linux-musl
                    elif glibc_before_2_34; then
                        echo x86_64-unknown-linux-musl
                    else
                        echo x86_64-unknown-linux-gnu
                    fi
                    ;;
                aarch64 | arm64) echo aarch64-unknown-linux-musl ;;
                *) die "no prebuilt probbit for Linux on $arch yet; build from source: cargo install --git https://github.com/$REPO probbit-cli" ;;
            esac
            ;;
        MINGW* | MSYS* | CYGWIN* | Windows_NT) die "on Windows use install.ps1 (PowerShell)" ;;
        *) die "no prebuilt probbit for $os; build from source: cargo install --git https://github.com/$REPO probbit-cli" ;;
    esac
}

latest_tag() {
    api="https://api.github.com/repos/$REPO/releases/latest"
    fetch "$api" "$tmp/latest.json" || die "could not look up the latest release at $api (set PROBBIT_VERSION)"
    sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$tmp/latest.json" | head -n 1
}

main() {
    base="${PROBBIT_DOWNLOAD_BASE:-https://github.com/$REPO/releases/download}"
    base="${base%/}"
    target="${PROBBIT_TARGET:-}"
    if [ -z "$target" ]; then target=$(detect_target) || exit 1; fi

    tmp=$(mktemp -d 2> /dev/null || mktemp -d -t probbit)
    trap 'rm -rf "$tmp"' EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    version="${PROBBIT_VERSION:-}"
    if [ -z "$version" ]; then
        [ -z "${PROBBIT_DOWNLOAD_BASE:-}" ] || die "set PROBBIT_VERSION (e.g. v0.14.0) together with PROBBIT_DOWNLOAD_BASE"
        version=$(latest_tag) || exit 1
        [ -n "$version" ] || die "no published release found (set PROBBIT_VERSION)"
    fi
    case "$version" in v*) ;; *) version="v$version" ;; esac
    case "$version" in *[!A-Za-z0-9._+-]* | v) die "invalid release version: $version" ;; esac
    case "$target" in *[!A-Za-z0-9_-]* | '') die "invalid target: $target" ;; esac

    name="probbit-$version-$target"
    asset="$name.tar.gz"
    url="$base/$version/$asset"
    say "probbit install: $version for $target"
    say "  fetching $url"
    fetch "$url" "$tmp/$asset" || die "download failed: $url"
    fetch "$url.sha256" "$tmp/$asset.sha256" || die "download failed: $url.sha256"

    want=$(cut -d ' ' -f 1 < "$tmp/$asset.sha256" | tr 'ABCDEF' 'abcdef')
    got=$(sha256 "$tmp/$asset" | tr 'ABCDEF' 'abcdef')
    case "$want" in *[!0-9a-f]* | '') die "unreadable checksum file $asset.sha256" ;; esac
    [ "${#want}" -eq 64 ] || die "unreadable checksum file $asset.sha256"
    [ "$want" = "$got" ] || die "checksum mismatch for $asset: expected $want, got $got (nothing was installed)"
    say "  sha256 ok  $got"

    tar -xzf "$tmp/$asset" -C "$tmp" || die "could not unpack $asset"
    [ -f "$tmp/$name/probbit" ] || die "$asset has no $name/probbit"
    # Check the candidate before replacing an existing working install (wrong architecture, corrupt executable, etc.).
    chmod 755 "$tmp/$name/probbit" || die "could not make the downloaded binary executable"
    ran=$("$tmp/$name/probbit" version 2>&1) || die "downloaded binary does not run here; existing install unchanged: $ran"

    if [ -n "${PROBBIT_INSTALL_DIR:-}" ]; then
        dir="$PROBBIT_INSTALL_DIR"
    else
        dir="${HOME:?HOME is not set; set PROBBIT_INSTALL_DIR}/.local/bin"
    fi
    mkdir -p "$dir" 2> /dev/null || die "cannot create $dir (set PROBBIT_INSTALL_DIR to a directory you can write)"
    [ -w "$dir" ] || die "cannot write to $dir (set PROBBIT_INSTALL_DIR to a directory you can write; this script never uses sudo)"
    cp "$tmp/$name/probbit" "$dir/.probbit.$$" && chmod 755 "$dir/.probbit.$$" && mv -f "$dir/.probbit.$$" "$dir/probbit" \
        || die "could not install into $dir"
    say "  installed  $dir/probbit ($ran)"
    # the manual page (archives from 0.8.2 carry docs/probbit.1): never fatal, never outside PROBBIT_MAN_DIR, the install
    # prefix or HOME; a PROBBIT_INSTALL_DIR of <prefix>/bin keeps it in <prefix>/share/man/man1
    if [ -f "$tmp/$name/docs/probbit.1" ]; then
        mandir="${PROBBIT_MAN_DIR:-}"
        if [ -z "$mandir" ] && [ -n "${PROBBIT_INSTALL_DIR:-}" ]; then
            case "$dir" in ?*/bin | ?*/bin/) mandir="${dir%/}"; mandir="${mandir%/bin}/share/man/man1" ;; esac
        fi
        where=""
        if [ -z "$mandir" ] && [ -n "${HOME:-}" ]; then
            mandir="$HOME/.local/share/man/man1"
            # a PROBBIT_INSTALL_DIR that is not <prefix>/bin has no man directory of its own: say that the page left it
            if [ -n "${PROBBIT_INSTALL_DIR:-}" ]; then where="; outside PROBBIT_INSTALL_DIR, PROBBIT_MAN_DIR to choose"; fi
        fi
        if [ -n "$mandir" ] && mkdir -p "$mandir" 2> /dev/null && cp "$tmp/$name/docs/probbit.1" "$mandir/probbit.1" 2> /dev/null; then
            say "  installed  $mandir/probbit.1 (man probbit$where)"
        else
            say "  skipped    the manual page (set PROBBIT_MAN_DIR to a directory you can write; or: probbit man)"
        fi
    fi

    cmd=probbit
    case ":$PATH:" in
        *":$dir:"*) ;;
        *)
            cmd="$dir/probbit"
            say ""
            # the file the user's shell reads: zsh ~/.zshrc; bash ~/.bash_profile on macOS (Terminal opens login shells),
            # ~/.bashrc elsewhere; any other shell ~/.profile
            case "${SHELL:-}" in
                */zsh) profile='~/.zshrc' ;;
                */bash) if [ "$(uname -s 2> /dev/null)" = Darwin ]; then profile='~/.bash_profile'; else profile='~/.bashrc'; fi ;;
                *) profile='~/.profile' ;;
            esac
            say "$dir is not on your PATH. Add it in $profile:"
            say "  export PATH=\"$dir:\$PATH\""
            ;;
    esac
    say ""
    # At a terminal, the banner is probbit's own hero screen (its theme: NO_COLOR or PROBBIT_THEME=plain turn the colour off);
    # releases before 0.3.0 have none (a bare `probbit` exits 2 there), and a pipe gets the plain next steps
    if [ -t 1 ] && "$dir/probbit" 2> /dev/null; then return 0; fi
    say "Next:"
    say "  $cmd version"
    say "  $cmd demo --tasks 12 | $cmd decide --pretty"
    say "  $cmd stats --pretty"
}

main "$@"
