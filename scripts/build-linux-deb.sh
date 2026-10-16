#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in python3 dpkg-shlibdeps dpkg-deb dpkg readelf npm cargo; do
  command -v "$tool" >/dev/null || { echo "Required build tool missing: $tool" >&2; exit 1; }
done
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
# Extra arguments are forwarded only to Cargo, e.g. --config for a local mirror.
npm exec tauri -- build --no-bundle -- --locked "$@"
python3 scripts/linux_deb.py config src-tauri/target/release/ai-selection-assistant > "$scratch/runtime.json"
npm exec tauri -- bundle --bundles deb --config "$scratch/runtime.json"
version=$(python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
architecture=$(dpkg --print-architecture)
package="src-tauri/target/release/bundle/deb/AI Selection Assistant_${version}_${architecture}.deb"
bash scripts/verify-linux-deb.sh "$package"
