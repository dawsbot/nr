#!/bin/sh
set -e

REPO="dawsbot/nr"
BINARY="nr"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS="linux" ;;
    Darwin) OS="darwin" ;;
    MINGW*|MSYS*|CYGWIN*) OS="windows" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

if [ "$OS" = "windows" ]; then
    ASSET="${BINARY}-${OS}-${ARCH}.zip"
    BINARY="${BINARY}.exe"
else
    ASSET="${BINARY}-${OS}-${ARCH}.tar.gz"
fi

URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

echo "Downloading nr for ${OS}-${ARCH}..."

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

if ! curl -fsSL "$URL" -o "$TMPDIR/$ASSET"; then
    echo "Failed to download from $URL"
    echo "Make sure a release exists for your platform: ${OS}-${ARCH}"
    exit 1
fi

cd "$TMPDIR"
if [ "$OS" = "windows" ]; then
    unzip -q "$ASSET"
else
    tar -xzf "$ASSET"
fi

# Find install location
if [ "$OS" = "windows" ]; then
    INSTALL_DIR="$HOME/bin"
    mkdir -p "$INSTALL_DIR"
elif [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
elif [ -w "$HOME/.local/bin" ]; then
    INSTALL_DIR="$HOME/.local/bin"
else
    INSTALL_DIR="/usr/local/bin"
fi

echo "Installing to ${INSTALL_DIR}..."

if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY" "$INSTALL_DIR/$BINARY"
else
    sudo mv "$BINARY" "$INSTALL_DIR/$BINARY"
fi

chmod +x "$INSTALL_DIR/$BINARY"

echo ""
echo "✓ Installed nr to ${INSTALL_DIR}/${BINARY}"
echo ""
echo "Run 'nr' to list available scripts"
