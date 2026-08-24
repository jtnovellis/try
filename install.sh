#!/bin/sh
# try install script — downloads a pre-compiled binary from GitHub releases
# Usage: curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh
#
# Alternatively: curl -sL https://raw.githubusercontent.com/jtnovellis/try/main/install.sh | sh -s -- --path ~/my-tries

set -e

REPO="jtnovellis/try"
GITHUB_API="https://api.github.com/repos/$REPO/releases/latest"
BIN_NAME="try"
INSTALL_DIR="${HOME}/.local/bin"
TRIES_PATH="${TRY_PATH:-${HOME}/src/tries}"

# Parse arguments
while [ $# -gt 0 ]; do
    case "$1" in
        --path) TRIES_PATH="$2"; shift 2 ;;
        --dir)  INSTALL_DIR="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  PLATFORM="linux" ;;
    Darwin) PLATFORM="darwin" ;;
    *)
        echo "Error: unsupported OS: $OS"
        echo "Please build from source instead:"
        echo "  git clone https://github.com/$REPO.git"
        echo "  cd try && make build"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_NAME="x86_64" ;;
    arm64|aarch64) ARCH_NAME="aarch64" ;;
    *)
        echo "Error: unsupported architecture: $ARCH"
        echo "Please build from source instead:"
        echo "  git clone https://github.com/$REPO.git"
        echo "  cd try && make build"
        exit 1
        ;;
esac

ARTIFACT="try-${ARCH_NAME}-${PLATFORM}"

echo "Installing try for ${ARCH_NAME}-${PLATFORM}..."

# Fetch the download URL from GitHub API
if command -v curl >/dev/null 2>&1; then
    DOWNLOAD_URL=$(curl -sL "$GITHUB_API" | grep "browser_download_url.*$ARTIFACT" | cut -d'"' -f4)
elif command -v wget >/dev/null 2>&1; then
    DOWNLOAD_URL=$(wget -qO- "$GITHUB_API" | grep "browser_download_url.*$ARTIFACT" | cut -d'"' -f4)
else
    echo "Error: neither curl nor wget is installed"
    exit 1
fi

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: no pre-compiled binary found for ${ARCH_NAME}-${PLATFORM}"
    echo "Please build from source instead:"
    echo "  git clone https://github.com/$REPO.git"
    echo "  cd try && make build && make install"
    exit 1
fi

echo "Downloading from $DOWNLOAD_URL..."

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download and install
TMPFILE=$(mktemp)
if command -v curl >/dev/null 2>&1; then
    curl -sL "$DOWNLOAD_URL" -o "$TMPFILE"
else
    wget -q "$DOWNLOAD_URL" -O "$TMPFILE"
fi

chmod +x "$TMPFILE"
mv "$TMPFILE" "${INSTALL_DIR}/${BIN_NAME}"

echo ""
echo "Installed to ${INSTALL_DIR}/${BIN_NAME}"
echo ""

# Detect shell for instructions
SHELL_NAME="$(basename "${SHELL:-/bin/sh}")"

case "$SHELL_NAME" in
    fish)
        echo "Add this to your ~/.config/fish/config.fish:"
        echo ""
        echo "  ${INSTALL_DIR}/${BIN_NAME} init ${TRIES_PATH} | source"
        ;;
    *)
        echo "Add this to your shell config (~/.zshrc or ~/.bashrc):"
        echo ""
        echo "  eval \"\$(${INSTALL_DIR}/${BIN_NAME} init ${TRIES_PATH})\""
        ;;
esac

echo ""
echo "Then restart your shell or source the config file."
echo "Done!"
