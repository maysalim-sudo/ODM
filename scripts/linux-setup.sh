#!/usr/bin/env bash
# Installs what building ODM on Debian/Ubuntu (or WSL) needs: the GTK/WebKit libraries
# Tauri links against, rpm for the .rpm package, Rust and Node with pnpm.
# Run it as your normal user; it asks for your password once, for apt.
set -euo pipefail

sudo apt-get update
sudo apt-get install -y \
  build-essential curl wget file pkg-config git \
  libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev \
  libssl-dev libxdo-dev patchelf rpm

# Rust, for this user only.
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
. "$HOME/.cargo/env"
rustup show >/dev/null

# Node 24 and pnpm, for this user only (no system Node needed).
if [ ! -d "$HOME/.nvm" ]; then
  curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash
fi
export NVM_DIR="$HOME/.nvm"
. "$NVM_DIR/nvm.sh"
nvm install 24
corepack enable
corepack prepare pnpm@latest --activate

echo
echo "Done: $(rustc --version), node $(node --version), pnpm $(pnpm --version)."
