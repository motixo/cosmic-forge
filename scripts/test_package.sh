#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-only
# Runs only inside a disposable distribution container, never on the host.
set -eu
test -f /.dockerenv || { echo 'Use a disposable Docker container.' >&2; exit 1; }
case "$1" in
    deb)
        apt-get update
        DEBIAN_FRONTEND=noninteractive apt-get install -y /release/*.deb python3-pil python3-xlib xvfb dbus-x11 mesa-vulkan-drivers desktop-file-utils
        ;;
    rpm)
        dnf install -y --allowerasing --setopt=install_weak_deps=False /release/*.rpm python3-pillow python3-xlib xorg-x11-server-Xvfb mesa-dri-drivers mesa-vulkan-drivers dbus-daemon desktop-file-utils
        ;;
    arch)
        pacman -Syu --noconfirm
        pacman -S --needed --noconfirm glibc gcc-libs libxkbcommon wayland libglvnd fontconfig python curl python-pillow python-xlib xorg-server-xvfb mesa vulkan-swrast dbus desktop-file-utils
        pacman -U --noconfirm /release/*.pkg.tar.zst
        ;;
    *) exit 2 ;;
esac
desktop-file-validate /usr/share/applications/io.github.motixo.CosmicForge.desktop
case "$1" in
    deb) dpkg --verify cosmic-forge ;;
    rpm) rpm -V cosmic-forge ;;
    arch) pacman -Qkk cosmic-forge ;;
esac
useradd --create-home forge
mkdir -p /results
chmod 777 /results
runuser -u forge -- python3 /src/tests/qa_release.py --binary /usr/bin/cosmic-forge --output /results/gui
case "$1" in
    deb) apt-get remove -y cosmic-forge ;;
    rpm) dnf remove -y cosmic-forge ;;
    arch) pacman -R --noconfirm cosmic-forge ;;
esac
test ! -e /usr/bin/cosmic-forge
test -f /results/gui/config/cosmic/com.system76.CosmicComp/v1/glass_effect
echo 'PASS native package installation and removal; preferences retained'
