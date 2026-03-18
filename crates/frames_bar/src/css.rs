//! CSS theme loading for the Frames bar.
//!
//! Loads CSS into a [`gtk::CssProvider`] and applies it globally. The load
//! priority chain is:
//! 1. CLI `--theme <name>` override → `~/.config/frames/themes/<name>.css`
//! 2. `bar.theme` config field → `~/.config/frames/themes/<name>.css`
//! 3. `bar.css` config field → raw filesystem path
//! 4. Built-in default theme (`themes/default.css`, compiled in at build time)
//!
//! A failure to load a user theme is a warning, not a fatal error; the bar
//! always continues with the built-in default.

use std::path::{Path, PathBuf};

use gtk::prelude::*;

// ── ThemeSource ───────────────────────────────────────────────────────────────

/// Describes where the active CSS theme should be loaded from.
///
/// Passed to [`load_theme`] to determine the CSS source in priority order.
/// Use [`ThemeSource::Default`] when neither a named theme nor a raw path is
/// configured.
#[derive(Clone, Copy)]
pub enum ThemeSource<'a> {
    /// Named theme: resolved to `~/.config/frames/themes/<name>.css`.
    ///
    /// The name must not contain `/` or `..`. An invalid name falls through
    /// to the built-in default with a warning.
    #[allow(dead_code)] // preserved; callers now pre-resolve names via resolve_active_css_path
    Named(&'a str),
    /// Raw filesystem path (backward-compatible with the `bar.css` config field).
    Path(&'a Path),
    /// Use the compiled-in built-in default theme.
    Default,
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Returns the absolute path for a named theme file.
///
/// Resolves to `~/.config/frames/themes/<name>.css` using the `HOME`
/// environment variable. The returned path may not exist on disk — callers
/// should probe before passing to [`load_theme`].
///
/// Returns an empty [`PathBuf`] and logs a warning if `name` contains `/`
/// or `..` (path traversal guard) or if `HOME` is unset.
///
/// # Parameters
/// - `name` — theme name, e.g. `"dark"` or `"gruvbox"`. Must match
///   `[a-z0-9_-]+`; anything containing `/` or `..` is rejected.
#[must_use]
pub fn resolve_theme_path(name: &str) -> PathBuf {
    if name.contains('/') || name.contains("..") {
        tracing::warn!(
            name,
            "theme name contains path separators; ignoring to prevent path traversal"
        );
        return PathBuf::new();
    }
    let Ok(home) = std::env::var("HOME") else {
        tracing::warn!("HOME is not set; cannot resolve named theme path");
        return PathBuf::new();
    };
    PathBuf::from(home)
        .join(".config")
        .join("frames")
        .join("themes")
        .join(format!("{name}.css"))
}

/// Returns the effective theme name after applying the system colour-scheme preference.
///
/// If `GtkSettings::gtk-application-prefer-dark-theme` is `true`, appends
/// `-dark` to `name` and checks whether that variant file exists. If `false`,
/// appends `-light`. Falls back to the unmodified `name` when no matching
/// variant file is found or when GTK settings are unavailable.
///
/// Must be called after `gtk::init()`.
///
/// # Parameters
/// - `name` — base theme name, e.g. `"gruvbox"`.
#[must_use]
pub fn resolve_theme_variant(name: &str) -> String {
    let dark = gtk::Settings::default().is_some_and(|s| s.is_gtk_application_prefer_dark_theme());
    let suffix = if dark { "-dark" } else { "-light" };
    let candidate = resolve_theme_path(&format!("{name}{suffix}"));
    if !candidate.as_os_str().is_empty() && candidate.exists() {
        format!("{name}{suffix}")
    } else {
        name.to_string()
    }
}

// ── Provider management ───────────────────────────────────────────────────────

/// Load a CSS theme from the given source into a new [`gtk::CssProvider`].
///
/// - `ThemeSource::Named(name)` — resolved via [`resolve_theme_path`]; falls
///   back to built-in on any file error.
/// - `ThemeSource::Path(path)` — loaded directly; falls back to built-in on
///   any file error.
/// - `ThemeSource::Default` — always uses the compiled-in built-in theme.
///
/// The built-in CSS (`themes/default.css`) is compiled in via `include_bytes!`
/// and always valid — its `.expect()` is the one accepted exception to the
/// no-expect rule (`CODING_STANDARDS §3.2`).
///
/// Returns the loaded provider — apply to the screen with [`apply_provider`].
#[must_use]
pub fn load_theme(source: ThemeSource<'_>) -> gtk::CssProvider {
    let provider = gtk::CssProvider::new();

    let path: Option<PathBuf> = match source {
        ThemeSource::Named(name) => {
            let p = resolve_theme_path(name);
            if p.as_os_str().is_empty() {
                None
            } else {
                Some(p)
            }
        }
        ThemeSource::Path(p) => Some(p.to_path_buf()),
        ThemeSource::Default => None,
    };

    if let Some(ref p) = path {
        match provider.load_from_path(p.to_str().unwrap_or("")) {
            Ok(()) => {
                tracing::debug!(path = %p.display(), "user CSS loaded");
                return provider;
            }
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    error = %e,
                    "user CSS failed to load; using built-in theme"
                );
            }
        }
    }

    // Built-in CSS is always valid — panic here is a programming error.
    provider
        .load_from_data(include_bytes!("../themes/default.css"))
        .expect("built-in CSS is always valid");
    provider
}

/// Apply a [`gtk::CssProvider`] globally at application priority.
///
/// Uses [`gtk::StyleContext::add_provider_for_screen`] with
/// `STYLE_PROVIDER_PRIORITY_APPLICATION` so user CSS takes precedence over
/// GTK theme defaults.
///
/// # Panics
///
/// Panics if no default GDK screen is available (this only happens before
/// GTK is initialized, which is always done before this function is called).
pub fn apply_provider(provider: &gtk::CssProvider) {
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::default().expect("default GDK screen must exist after gtk::init"),
        provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Remove a previously-applied [`gtk::CssProvider`] from the default screen.
///
/// Used during theme hot-reload to cleanly replace the active theme without
/// provider stacking. Must be called with the exact object reference that was
/// passed to [`apply_provider`], and must be called on the GTK main thread.
///
/// # Panics
///
/// Panics if no default GDK screen is available — same conditions as
/// [`apply_provider`].
pub fn remove_provider(provider: &gtk::CssProvider) {
    gtk::StyleContext::remove_provider_for_screen(
        &gdk::Screen::default().expect("default GDK screen must exist after gtk::init"),
        provider,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_theme_path_correct() {
        // Set HOME to a known value so the test is reproducible.
        std::env::set_var("HOME", "/home/testuser");
        let p = resolve_theme_path("dark");
        assert_eq!(p, PathBuf::from("/home/testuser/.config/frames/themes/dark.css"));
    }

    #[test]
    fn resolve_theme_path_rejects_traversal_slash() {
        let p = resolve_theme_path("../evil");
        assert!(p.as_os_str().is_empty(), "path traversal with / should return empty PathBuf");
    }

    #[test]
    fn resolve_theme_path_rejects_traversal_dotdot() {
        let p = resolve_theme_path("foo..bar");
        assert!(p.as_os_str().is_empty(), "path traversal with .. should return empty PathBuf");
    }
}
