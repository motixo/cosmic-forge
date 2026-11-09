<p align="center"><img src="resources/icons/io.github.motixo.CosmicForge.svg" width="96" alt="COSMIC Forge icon"></p>
<h1 align="center">COSMIC Forge</h1>

Custom shaders for the **COSMIC desktop**. Apply and adjust effects,
or import your own JSON + `.frag` packs.

## Install

For Ubuntu/Pop!_OS, Fedora and Arch Linux. Requires COSMIC, Python 3.11+ and curl.

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh
```

Install the compositor from **Settings → Patches**, then log out and back in.

[Native packages](https://github.com/motixo/cosmic-forge/releases/latest) · [Create and import shaders](docs/adding-effects.md)

## Shader gallery

<table>
  <tr>
    <td align="center"><img src="resources/previews/crystal.webp" width="400" alt="Faceted refraction in a glass settings window"><br><strong>Crystal</strong></td>
    <td align="center"><img src="resources/previews/liquid-glass.webp" width="400" alt="Curved refraction around a glass window"><br><strong>Liquid Glass</strong></td>
  </tr>
  <tr>
    <td align="center"><img src="resources/previews/cyberspace.webp" width="400" alt="Cyan digital contours on a dark window"><br><strong>Cyberspace</strong></td>
    <td align="center"><img src="resources/previews/void.webp" width="400" alt="An infinity mirror inside a window"><br><strong>Void</strong></td>
  </tr>
  <tr>
    <td align="center"><img src="resources/previews/rain-glass.webp" width="400" alt="Raindrops sliding down a glass window"><br><strong>Rain Glass</strong></td>
    <td align="center"><img src="resources/previews/snow.webp" width="400" alt="Snow falling and settling on windows"><br><strong>Snow</strong></td>
  </tr>
  <tr>
    <td align="center"><img src="resources/previews/aero-glass.webp" width="400" alt="Blue glass with soft diagonal reflections"><br><strong>Aero Glass</strong></td>
    <td align="center"><img src="resources/previews/mica.webp" width="400" alt="Matte dark window tinted by a starry wallpaper"><br><strong>Mica</strong></td>
  </tr>
  <tr>
    <td align="center"><img src="resources/previews/minecraft-glass.webp" width="400" alt="Pixel reflections on glass over a Minecraft scene"><br><strong>Minecraft Glass</strong></td>
    <td align="center"><img src="resources/previews/ember.webp" width="400" alt="Flames rising behind a glass window"><br><strong>Ember</strong></td>
  </tr>
</table>

## Update reminders

Enable **Settings → Updates → Automatic update reminders** to check the app,
engine and shaders whenever Forge opens and every hour while it stays open.

## Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/motixo/cosmic-forge/main/install.sh | sh -s -- --uninstall
```

GPL-3.0-only
