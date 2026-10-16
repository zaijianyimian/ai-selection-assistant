#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 1 ]]; then echo "Usage: $0 PACKAGE.deb" >&2; exit 1; fi
script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
exec python3 "$script_dir/linux_deb.py" verify "$1"
