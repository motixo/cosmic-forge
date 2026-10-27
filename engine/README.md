# Forge glass renderer

This directory brings the existing glass implementation into the Forge source:
Crystal, Aero, and the integration with COSMIC's native Frosted Glass. Frosted
Glass remains the upstream compositor pipeline; its pinned source is fetched at
build time. No Penumbra, Veil, clock or font modifications are bundled.

Sources were imported from the local COSMIC Crystal project, corresponding to
`motixo/cosmic-crystal` 0.8.0 plus its working Aero changes. The code is GPL-3.0-only;
upstream COSMIC and Smithay keep their original notices. `patches/glass.patch`
contains the original optics, configuration watchers and theme integration.
`patches/runtime.patch`, `runtime.rs` and `loader.rs` add ABI 1 shader packs.
The app's `shaders/` submodule holds the editable Crystal/Aero sources and pack
publisher. The GLSL embedded in `patches/glass.patch` remains the ABI 1 fallback
snapshot; downloadable shader updates do not modify this renderer patch.
The existing render fixtures and their checks are retained under `tests/`.

## Build independently

```sh
python engine/build.py --cosmic-version 1.10.0
# Or 1.11.0; optionally reuse an upstream checkout and Cargo target cache:
python engine/build.py --cosmic-version 1.10.0 --source /path/cosmic-comp \
  --target-dir /path/cargo-cache --offline
```

`release.json` pins upstream commits. Prepared source goes under `build/`, never
into tracked source. Unmarked work directories are refused; changed patch hashes
require a fresh work directory. Cargo uses the upstream lockfile. This builder
produces only the compositor, not a replacement COSMIC Settings. It does not
install, change launchers, terminate processes or restart the user's desktop.
The resulting executable is `engine/build/bin/cosmic-comp`. Forge settings provides
**Install / update compositor** and **Restore original COSMIC compositor**.
This setup currently supports Arch Linux and derivatives with matching
COSMIC 1.10.0/1.11.0 compositor, Settings, panel and greeter packages. The tools and
pinned patches are bundled in the native executable and extracted to its cache.
No global Python package is installed. Native build dependencies must be present.

For a previously built binary, the separate helper can also be used directly:

```sh
python engine/install.py --binary "$PWD/engine/build/bin/cosmic-comp"
python engine/install.py --restore
```

The helper validates the binary's ABI marker and upstream revision, keeps immutable
versioned binaries and preserves the original launcher. Its package guards fall back
to the previous compositor after package upgrades or when COSMIC_FORGE_DISABLE=1.
Restore removes the verified user compositor launcher and Forge installation
record, selecting the original system `/usr/bin/cosmic-comp` at the next login.
It also handles recorded legacy Crystal launchers left by older Restore actions.
It checks that the native system binary is available before changing anything.
Old renderer binaries, previous-launcher backups, preferences, packs and the
legacy Settings installation record remain available. Existing managed Crystal
launchers are adopted only when their recorded contents match exactly. Unknown
or modified launchers are preserved. Neither Settings nor
system-owned compositor files are overwritten. Install/restore require the next
login to change the renderer; the current desktop session is never terminated.

Dependencies match the pinned COSMIC compositor (Rust/C toolchains, Wayland,
X11, libinput, libseat, udev, libdisplay-info and systemd development libraries).
For isolated software-rendered tests, install Xvfb, python-xlib, Pillow, Cairo,
Wayland scanner and cosmic-bg:

```sh
python engine/build_test_surface.py
python engine/preview.py --headless --effect aero \
  --compositor "$PWD/engine/build/bin/cosmic-comp" --run-dir "$PWD/build/glass-preview"
# In another terminal, while that preview is running:
CRYSTAL_QA_DIR="$PWD/build/glass-preview" python engine/tests/verify_packs.py
CRYSTAL_QA_DIR="$PWD/build/glass-preview" python engine/tests/verify_aero.py
touch build/glass-preview/stop
```

These fixtures use private XDG directories, display and D-Bus. They never alter
real desktop preferences. The pack check verifies real output changes from a new
effect id, a custom float control, live source replacement, invalid GLSL fallback
and recovery. Broader original Crystal checks remain available in `tests/`.

## Progress and repeated installation

The setup/build helpers emit `FORGE_PROGRESS:<stage>` lines on stdout. Forge
forwards these events from the background worker to the native animated progress
bar; Cargo/Git diagnostics remain in the build log. Restore and completion have
their own states. Failed operations stop the progress animation and show the error.

A matching installation is detected before source preparation or Cargo runs. This
check requires the same COSMIC version, exact captured package versions and
`renderer_revision` from `release.json`, plus a matching launcher and an executable
versioned binary with its recorded checksum. Initial Forge records without a
revision represent revision 1. Missing binaries need repair; modified binaries
and launchers are preserved and reported as errors. Increment `renderer_revision`
when changing the compositor patches/runtime loader; app-only UI or installer
changes do not require rebuilding the compositor.
