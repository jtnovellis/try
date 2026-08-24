#!/bin/sh
# try install script — downloads a pre-compiled binary from GitHub releases
# Usage: curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh
#
# Options:
#   --path <dir>      Where try should keep your tries (used in the printed setup hint)
#   --dir <dir>       Where to install the binary (default: ~/.local/bin)
#   --version <tag>   Install a specific release tag instead of the latest
#
# Example:
#   curl -sL .../install.sh | sh -s -- --path ~/my-tries --dir /usr/local/bin

set -eu

REPO="jtnovellis/try"
API="https://api.github.com/repos/$REPO"
BIN_NAME="try"
INSTALL_DIR="${TRY_INSTALL_DIR:-${HOME}/.local/bin}"
TRIES_PATH="${TRY_PATH:-${HOME}/src/tries}"
VERSION="${TRY_VERSION:-latest}"

TMPDIR_INSTALL=""
cleanup() {
    [ -n "$TMPDIR_INSTALL" ] && rm -rf "$TMPDIR_INSTALL"
    return 0
}
trap cleanup EXIT INT TERM

die() {
    echo "Error: $*" >&2
    exit 1
}

build_from_source_hint() {
    echo "" >&2
    echo "You can build from source instead:" >&2
    echo "  git clone https://github.com/$REPO.git" >&2
    echo "  cd try && make build && make install" >&2
}

# --- Parse arguments -------------------------------------------------------
while [ $# -gt 0 ]; do
    case "$1" in
        --path)
            [ $# -ge 2 ] || die "--path requires a directory"
            TRIES_PATH="$2"; shift 2 ;;
        --dir)
            [ $# -ge 2 ] || die "--dir requires a directory"
            INSTALL_DIR="$2"; shift 2 ;;
        --version)
            [ $# -ge 2 ] || die "--version requires a release tag"
            VERSION="$2"; shift 2 ;;
        -h|--help)
            cat <<'USAGE'
try install script - downloads a pre-compiled binary from GitHub releases

Usage: curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh

Options:
  --path <dir>      Where try should keep your tries (used in the setup hint)
  --dir <dir>       Where to install the binary (default: ~/.local/bin)
  --version <tag>   Install a specific release tag instead of the latest
  -h, --help        Show this message

Environment: TRY_PATH, TRY_INSTALL_DIR, TRY_VERSION
USAGE
            exit 0 ;;
        *)
            die "unknown option: $1" ;;
    esac
done

# --- Pick a downloader -----------------------------------------------------
if command -v curl >/dev/null 2>&1; then
    DOWNLOADER="curl"
elif command -v wget >/dev/null 2>&1; then
    DOWNLOADER="wget"
else
    die "neither curl nor wget is installed"
fi

# fetch <url> <dest>  — fails on HTTP errors, not just transport errors
fetch() {
    if [ "$DOWNLOADER" = "curl" ]; then
        curl -fsSL --retry 3 --connect-timeout 15 "$1" -o "$2"
    else
        wget -q --tries=3 --timeout=15 "$1" -O "$2"
    fi
}

# --- Detect platform -------------------------------------------------------
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="darwin" ;;
    *)
        echo "Error: unsupported OS: $OS" >&2
        build_from_source_hint
        exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH_NAME="x86_64" ;;
    arm64|aarch64) ARCH_NAME="aarch64" ;;
    *)
        echo "Error: unsupported architecture: $ARCH" >&2
        build_from_source_hint
        exit 1 ;;
esac

ARTIFACT="${BIN_NAME}-${ARCH_NAME}-${PLATFORM}"

echo "Installing try for ${ARCH_NAME}-${PLATFORM}..."

TMPDIR_INSTALL="$(mktemp -d)"
RELEASE_JSON="${TMPDIR_INSTALL}/release.json"

# --- Resolve the release ---------------------------------------------------
if [ "$VERSION" = "latest" ]; then
    RELEASE_URL="$API/releases/latest"
else
    RELEASE_URL="$API/releases/tags/$VERSION"
fi

# Our own message below is clearer than curl/wget's raw 404.
if ! fetch "$RELEASE_URL" "$RELEASE_JSON" 2>/dev/null; then
    if [ "$VERSION" = "latest" ]; then
        echo "Error: $REPO has no published releases yet." >&2
        echo "Nothing has been tagged, so there is no binary to download." >&2
    else
        echo "Error: release '$VERSION' not found in $REPO." >&2
    fi
    build_from_source_hint
    exit 1
fi

# GitHub pretty-prints its JSON, but split on commas too so we do not depend on it.
DOWNLOAD_URL="$(
    tr ',' '\n' < "$RELEASE_JSON" \
        | grep '"browser_download_url"' \
        | cut -d'"' -f4 \
        | grep "/${ARTIFACT}$" \
        | head -n 1 || true
)"

if [ -z "$DOWNLOAD_URL" ]; then
    TAG="$(tr ',' '\n' < "$RELEASE_JSON" | grep '"tag_name"' | cut -d'"' -f4 | head -n 1 || true)"
    echo "Error: release ${TAG:-$VERSION} has no pre-compiled binary for ${ARCH_NAME}-${PLATFORM}." >&2
    echo "Expected an asset named '${ARTIFACT}'." >&2
    build_from_source_hint
    exit 1
fi

# --- Download --------------------------------------------------------------
echo "Downloading $DOWNLOAD_URL..."
BINARY="${TMPDIR_INSTALL}/${BIN_NAME}"
fetch "$DOWNLOAD_URL" "$BINARY" || die "download failed"
[ -s "$BINARY" ] || die "downloaded file is empty"

# --- Verify checksum (when the release publishes SHA256SUMS) ---------------
SUMS_URL="$(
    tr ',' '\n' < "$RELEASE_JSON" \
        | grep '"browser_download_url"' \
        | cut -d'"' -f4 \
        | grep '/SHA256SUMS$' \
        | head -n 1 || true
)"

if [ -n "$SUMS_URL" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        SHA_CMD="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        SHA_CMD="shasum -a 256"
    else
        SHA_CMD=""
    fi

    if [ -n "$SHA_CMD" ] && fetch "$SUMS_URL" "${TMPDIR_INSTALL}/SHA256SUMS"; then
        EXPECTED="$(grep " \{1,2\}${ARTIFACT}\$" "${TMPDIR_INSTALL}/SHA256SUMS" | awk '{print $1}' | head -n 1 || true)"
        if [ -n "$EXPECTED" ]; then
            ACTUAL="$($SHA_CMD "$BINARY" | awk '{print $1}')"
            [ "$EXPECTED" = "$ACTUAL" ] || die "checksum mismatch for $ARTIFACT (expected $EXPECTED, got $ACTUAL)"
            echo "Checksum verified."
        fi
    fi
fi

# --- Install ---------------------------------------------------------------
if ! mkdir -p "$INSTALL_DIR" 2>/dev/null; then
    die "cannot create $INSTALL_DIR (try a writable --dir, or re-run with sudo)"
fi
if [ ! -w "$INSTALL_DIR" ]; then
    die "$INSTALL_DIR is not writable (try --dir \"\$HOME/.local/bin\", or re-run with sudo)"
fi

DEST="${INSTALL_DIR}/${BIN_NAME}"
chmod +x "$BINARY"
# Same-filesystem move so the swap is atomic; fall back to cp across devices.
mv "$BINARY" "$DEST" 2>/dev/null || { cp "$BINARY" "$DEST" && chmod +x "$DEST"; } \
    || die "failed to install to $DEST"

# --- Verify it actually runs ----------------------------------------------
if ! "$DEST" --version >/dev/null 2>&1; then
    die "installed binary at $DEST does not run on this machine"
fi

echo ""
echo "Installed $("$DEST" --version) to $DEST"
echo ""

case "$INSTALL_DIR" in
    *:*) ;;
    *)
        case ":${PATH}:" in
            *":${INSTALL_DIR}:"*) ;;
            *) echo "Note: ${INSTALL_DIR} is not on your PATH." ; echo "" ;;
        esac ;;
esac

# --- Shell setup hint ------------------------------------------------------
SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"

case "$SHELL_NAME" in
    fish)
        echo "Add this to your ~/.config/fish/config.fish:"
        echo ""
        echo "  ${DEST} init ${TRIES_PATH} | source"
        ;;
    *)
        echo "Add this to your shell config (~/.zshrc or ~/.bashrc):"
        echo ""
        echo "  eval \"\$(${DEST} init ${TRIES_PATH})\""
        ;;
esac

echo ""
echo "Then restart your shell or source the config file."
echo "Done!"
