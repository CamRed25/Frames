# Parapet — Platform Compatibility

> **Scope:** Supported Linux distributions, desktop environment requirements, X11 display system support, and runtime detection.
> **Last Updated:** Mar 17, 2026

---

## 1. Overview

Parapet targets Linux exclusively with the X11 display system. Cinnamon is the primary desktop environment target. Any EWMH-compliant window manager on X11 should work, but Cinnamon is the only one tested.

---

## 2. Platform Support Matrix

| Platform | Status | DE/WM | Notes |
|----------|--------|-------|-------|
| Linux Mint 21+ (Cinnamon) | **Primary target** | Cinnamon 5.x+ | Development and testing platform |
| Fedora 40+ (Cinnamon) | **Primary development** | Cinnamon from repos | Development machine |
| Ubuntu 24.04 + Cinnamon | Supported | Cinnamon installable | GTK3 matches |
| Arch Linux + Cinnamon | Supported | Rolling | Current GTK3 |
| Other EWMH WMs on X11 | Best effort | i3, Openbox, XFCE, etc. | May work; struts honored |
| GNOME on X11 | Best effort | GNOME | Different panel conventions |
| Wayland | Not supported | — | No layer-shell; future work |
| Windows / macOS | Not supported | — | X11-only |

**Primary target** means: tested before every release, all features verified.
**Supported** means: builds and expected to work, not tested on every release.
**Best effort** means: builds, may work, not actively tested.

---

## 3. Display System Requirements

### 3.1 X11 Required

Parapet requires X11. The bar uses:
- `gdk::WindowTypeHint::Dock` — requires X11 window type support
- `_NET_WM_STRUT_PARTIAL` — EWMH X11 property
- `_NET_WM_WINDOW_TYPE_DOCK` — EWMH X11 property
- `_NET_NUMBER_OF_DESKTOPS` / `_NET_CURRENT_DESKTOP` — workspace widget

These properties are X11/EWMH-specific. A Wayland compositor will not honor them.

### 3.2 EWMH Compliance Required

The window manager must be EWMH-compliant to respect the strut and dock type. Most Linux WMs are EWMH-compliant. A WM that does not honor `_NET_WM_STRUT_PARTIAL` will let windows overlap the bar.

### 3.3 DISPLAY Environment Variable

`DISPLAY` must be set when launching Parapet. GTK3 will fail to initialize without it. On a standard X11 desktop session this is always set.

```bash
# Verify before running
echo $DISPLAY  # Should print :0 or similar
```

---

## 4. GTK Version Requirements

| Requirement | Minimum | Notes |
|-------------|---------|-------|
| GTK | 3.22 | Oldest widely available GTK3 |
| GLib | 2.56 | Matches GTK 3.22 baseline |
| Cairo | 1.14 | GTK3 rendering backend |

Cinnamon itself requires GTK 3.22+. Any Cinnamon installation has a compatible GTK3.

**GTK4 is not used.** Cinnamon is a GTK3 desktop. Using GTK4 would require cross-toolkit process isolation, which adds unnecessary complexity.

---

## 5. Cinnamon-Specific Behavior

### 5.1 Panel Reserve Area

Cinnamon respects `_NET_WM_STRUT_PARTIAL`. When Parapet sets this property, Cinnamon:
- Prevents maximized windows from overlapping the bar
- Adjusts the work area for window placement

If the strut is not set correctly, maximized windows will cover the bar. See BAR_DESIGN.md §3 for the exact strut values.

### 5.2 Running Alongside Cinnamon's Panel

Parapet can run alongside Cinnamon's built-in panel. Set different screen positions (e.g., Parapet on top, Cinnamon panel on bottom) to avoid overlap. Both struts will be respected.

To hide Cinnamon's panel: right-click the panel → Properties → Auto-hide, or remove it entirely.

### 5.3 Cinnamon Themes

Cinnamon applies its own GTK theme to all GTK3 applications including Parapet. The `parapet.css` user stylesheet overrides Cinnamon theme properties for bar-specific elements. Use specific CSS selectors to avoid unintentionally overriding Cinnamon's own widgets.

---

## 6. Distro-Specific Notes

### 6.1 Fedora

Cinnamon is available in the Fedora repos:
```bash
sudo dnf install cinnamon
```

GTK3 dev headers for building:
```bash
sudo dnf install gtk3-devel
```

### 6.2 Ubuntu / Debian

Cinnamon is available in Ubuntu repos from 20.04+:
```bash
sudo apt install cinnamon-desktop-environment
```

GTK3 dev headers:
```bash
sudo apt install libgtk-3-dev
```

### 6.3 Arch Linux

```bash
sudo pacman -S cinnamon
sudo pacman -S gtk3  # dev headers included
```

### 6.4 NixOS

NixOS's non-standard FHS layout can cause issues with GTK theme loading and `pkg-config` detection. Nix packaging is not maintained. The standard `cargo build` should work if GTK3 is in the Nix environment.

### 6.5 SELinux (Fedora, RHEL)

SELinux may restrict X11 socket access in some profiles. If the bar fails to connect to X11, check SELinux audit logs:
```bash
sudo ausearch -m avc -ts recent
```

---

## 7. Minimum System Requirements

| Requirement | Minimum |
|-------------|---------|
| Linux kernel | Any (no kernel features required) |
| X11 server | Any (Xorg recommended) |
| GTK | 3.22 |
| EWMH-compliant WM | Required for strut support |
| RAM | Minimal — bar is a thin process |
| Display | X11 `DISPLAY` set |

---

## 8. Cross-References

| Topic | Standard |
|-------|----------|
| Governance and enforcement | [RULE_OF_LAW.md](RULE_OF_LAW.md) |
| X11 EWMH strut design | [BAR_DESIGN.md §3](BAR_DESIGN.md) |
| Build prerequisites | [BUILD_GUIDE.md §1](BUILD_GUIDE.md) |
| GTK3 conventions | [UI_GUIDE.md](UI_GUIDE.md) |
