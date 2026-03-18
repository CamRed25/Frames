//! Integration test: verify GTK3 initializes without panicking.
//!
//! `parapet_bar` is a binary-only crate, so this test validates the GTK3
//! initialization path that the bar relies on. Skips silently in headless
//! (CI) environments. This satisfies step 32 of the implementation plan.

#[test]
fn gtk_initializes_without_panic() {
    // Skip when no display is available — CI / headless environments.
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        eprintln!("SKIP: no display available");
        return;
    }

    // GTK init must succeed on any system with a running display server.
    gtk::init().expect("GTK init failed — is a display server running?");
}
