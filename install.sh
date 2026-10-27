#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
set -eu
command -v python3 >/dev/null || { echo 'Install Python 3.11+ first.' >&2; exit 1; }
command -v curl >/dev/null || { echo 'Install curl first.' >&2; exit 1; }
forge_installer_dir=$(mktemp -d)
trap 'rm -rf "$forge_installer_dir"' EXIT HUP INT TERM
curl --fail --silent --show-error --location --proto '=https' \
    https://raw.githubusercontent.com/motixo/cosmic-forge/main/scripts/easy_install.py \
    --output "$forge_installer_dir/install.py"
python3 "$forge_installer_dir/install.py" "$@"
