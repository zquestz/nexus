# Linux Desktop Integration

Files for desktop menu integration and application icon.

## Installation

```bash
# User installation
cp nexus.desktop ~/.local/share/applications/
cp nexus.svg ~/.local/share/icons/hicolor/scalable/apps/

# System-wide installation
sudo cp nexus.desktop /usr/share/applications/
sudo cp nexus.svg /usr/share/icons/hicolor/scalable/apps/
```

## Update Caches

```bash
# User
update-desktop-database ~/.local/share/applications/
gtk-update-icon-cache ~/.local/share/icons/hicolor/

# System-wide
sudo update-desktop-database /usr/share/applications/
sudo gtk-update-icon-cache /usr/share/icons/hicolor/
```

## Uninstall

```bash
# User
rm ~/.local/share/applications/nexus.desktop
rm ~/.local/share/icons/hicolor/scalable/apps/nexus.svg

# System-wide
sudo rm /usr/share/applications/nexus.desktop
sudo rm /usr/share/icons/hicolor/scalable/apps/nexus.svg
```
