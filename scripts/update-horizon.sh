#!/usr/bin/env bash
# Holt einen neuen Stand des Horizon-Quellcodes (Rust-Edition) und ersetzt rust-edition/horizon/.
# Aufruf: scripts/update-horizon.sh [commit-oder-branch]   (ohne Angabe: main)
# Danach in BuildMC unter "Rust" auf "Neu bauen" -- der Status zeigt "Gebaut (veraltet)".
set -euo pipefail
cd "$(dirname "$0")/.."

ref="${1:-main}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git clone --quiet --filter=blob:none https://github.com/horizon-rust/horizon.git "$tmp/horizon"
git -C "$tmp/horizon" checkout --quiet "$ref"
stand="$(git -C "$tmp/horizon" log -1 --format='%H %cd')"

# Lizenz pruefen, bevor irgendetwas ersetzt wird.
grep -q "^MIT License" "$tmp/horizon/LICENSE" || { echo "LICENSE ist nicht mehr MIT -- abgebrochen, bitte prüfen" >&2; exit 1; }

rsync -a --delete --exclude .git --exclude .local "$tmp/horizon/" rust-edition/horizon/
sed -i "s|^- Übernommener Stand: .*|- Übernommener Stand: \`$stand\`|" rust-edition/VENDORED.md
echo "Horizon aktualisiert auf $stand"
