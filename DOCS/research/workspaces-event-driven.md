# Research: WorkspacesWidget X11 Event-Driven Updates
**Date:** 2026-03-18
**Status:** Findings complete — decision recommended — ready to close

## Question

Can `WorkspacesWidget` be made event-driven on X11 property changes
(`_NET_CURRENT_DESKTOP`, `_NET_NUMBER_OF_DESKTOPS`, `_NET_DESKTOP_NAMES`) to
eliminate the self-polling `glib::timeout_add_local` timer?

---

## Summary

True event-driven workspace updates on X11 are **technically feasible but
require unsafe FFI** because `gdk_window_add_filter()` — the C API for
intercepting raw `XEvent` messages — is not exposed in the gdk-rs 0.18 safe
bindings. A safe intermediate step is available: reduce the self-poll interval
from 500 ms to 100 ms (one-line change, zero risk, effectively instant from a
user perspective). The unsafe FFI approach is the correct long-term answer and
is well-defined; it should be planned as its own implementation task.

---

## API Constraint — Root Cause

The primary blocker is the `gdk-rs` binding gap:

| C API | gdk-rs 0.18 exposure |
|-------|----------------------|
| `gdk_window_add_filter(GdkWindow*, GdkFilterFunc, gpointer)` | **Not exposed** |
| `gdk_window_set_events(GdkWindow*, GdkEventMask)` | ✅ `gdk::Window::set_events(EventMask)` |
| `GDK_PROPERTY_CHANGE_MASK` | ✅ `gdk::EventMask::PROPERTY_CHANGE_MASK` |
| `GDK_EVENT_PROPAGATE` / `GDK_FILTER_*` | **Not exposed** |

`WindowExtManual` in `gdk 0.18` provides only:
`set_user_data`, `user_data`, `default_root_window`,
`offscreen_{render_to_window,is_embedder,get_embedder}`,
`pixbuf_{get_from_window,get_from_surface}`, `set_background_pattern`.

There is no `add_filter`, no `connect_property_notify` for root-window X11
property events, and no first-class GTK signal for EWMH property changes.

---

## Findings

### How X11 PropertyNotify on the root window works

Any X11 client can watch for property changes on the root window by:

1. Calling `XSelectInput(display, root, PropertyChangeMask)`
2. Receiving `PropertyNotify` events in the event loop
3. Checking `event.atom` against the `_NET_CURRENT_DESKTOP` / `_NET_NUMBER_OF_DESKTOPS` / `_NET_DESKTOP_NAMES` atoms

GDK wraps this with `gdk_window_set_events(root, GDK_PROPERTY_CHANGE_MASK)` and
then an event filter (`gdk_window_add_filter`) that intercepts raw `XEvent`
structs and converts them into `GdkEvent` structs (specifically `GdkEventProperty`).

---

### Option A — Unsafe FFI to `gdk_window_add_filter` (true event-driven)

GDK 3 exposes:

```c
void gdk_window_add_filter(
    GdkWindow       *window,
    GdkFilterFunc    function,
    gpointer         data
);

typedef GdkFilterReturn (*GdkFilterFunc)(
    GdkXEvent *xevent,   // actually XEvent*
    GdkEvent  *event,
    gpointer   data
);
```

The Rust implementation in `crates/frames_bar`:

```rust
use gdk::prelude::*;
use glib::translate::ToGlibPtr;
use std::os::raw::c_void;

extern "C" {
    fn gdk_window_add_filter(
        window: *mut c_void,   // GdkWindow *
        function: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
        data: *mut c_void,
    );
}

// GdkFilterReturn: GDK_FILTER_CONTINUE = 0, GDK_FILTER_REMOVE = 1
unsafe extern "C" fn workspace_event_filter(
    xevent_ptr: *mut c_void,
    _gdk_event: *mut c_void,
    data: *mut c_void,
) -> i32 {
    // data is a raw pointer to a glib::Sender<()> or similar channel
    // XEvent layout: first field is type (i32), second is serial (u64),
    // third is PropertyNotify sub-fields.
    // Use the `x11` crate for safe XEvent layout access:
    let ev = &*(xevent_ptr as *const x11::xlib::XEvent);
    let event_type = ev.get_type();
    if event_type == x11::xlib::PropertyNotify {
        let prop_ev = &ev.property;
        // Check atom matches one of the three EWMH atoms
        // (atoms were looked up during widget init via gdk::Atom::intern)
        let notify_atom = prop_ev.atom;
        let target: *const WorkspacesShared = data as *const WorkspacesShared;
        let shared = &*target;
        if shared.watched_atoms.contains(&notify_atom) {
            let _ = shared.tx.send(());
        }
    }
    0 // GDK_FILTER_CONTINUE
}
```

Activate the filter:

```rust
// During WorkspacesWidget init:
let root: gdk::Window = gdk::Display::default()
    .unwrap()
    .default_screen()
    .root_window();

root.set_events(root.events() | gdk::EventMask::PROPERTY_CHANGE_MASK);

let shared = Box::new(WorkspacesShared { watched_atoms, tx });
let data_ptr = Box::into_raw(shared) as *mut c_void;

unsafe {
    gdk_window_add_filter(
        root.to_glib_none().0 as *mut c_void,
        workspace_event_filter,
        data_ptr,
    );
}
```

On receipt of channel message, `refresh()` is called from the glib main thread.

**Dependencies needed:**

The `data_ptr` callback currently uses EWMH atom values as `u64` identifiers.
To decode `XEvent` layout safely, the `x11` crate is needed:

```toml
# frames_bar/Cargo.toml
x11 = { workspace = true, features = ["xlib"] }
```

`x11` is already a transitive dependency (via `gdk`/`gtk` C libraries) but may
not be declared as a direct workspace dep. Must confirm in `Cargo.lock`.

**Precedent:** `crates/frames_bar/src/launcher.rs` already uses `unsafe` blocks
for X11 window operations. This pattern is consistent with the codebase.

**Safety requirements per `CODING_STANDARDS.md §8.2`:**
- Each `unsafe` block requires a `// SAFETY:` comment explaining the invariant
- `data_ptr`: must remain valid for the lifetime of the window filter;
  the `Box::into_raw` allocation must be paired with a `Box::from_raw` on teardown
  (call `gdk_window_remove_filter` in the widget's cleanup path)

**Pros:**
- Zero polling after setup. Workspace label updates within one glib event loop
  tick of the X11 property notification.
- No extra timer; the self-polling `glib::timeout_add_local` is removed entirely.

**Cons:**
- `unsafe` FFI surface (~30 lines). Must be reviewed carefully.
- Requires `x11` crate as a direct dep in `frames_bar/Cargo.toml`.
- Must implement `remove_filter` teardown or the dangling `data_ptr` will be
  called after the widget is dropped.
- X event filter fires for **every** X event until the filter checks the type —
  minor overhead, but `PropertyNotify` on the root window is low-frequency.

---

### Option B — `glib::unix_fd_add` on the X11 display file descriptor

```rust
let fd = gdk::Display::default()
    .unwrap()
    .connection_number();   // i32 — X11 socket fd

glib::unix_fd_add_local(fd, glib::IOCondition::IN, move |_, _| {
    // Drain XPending events; process PropertyNotify
    glib::ControlFlow::Continue
});
```

Inside the callback, `XPending` + `XNextEvent` (unsafe x11 calls) would be
needed to read events without blocking.

**Problem:** GDK is already reading from this fd in its own event dispatch. Both
the GDK event handler and the fd-add callback would race to read from the same
socket. This approach risks corrupting GDK's event queue state. **Not recommended.**

---

### Option C — Reduce poll interval from 500 ms to 100 ms (immediate safe fix)

Current default in `main.rs` (~line 628):

```rust
let interval_ms = workspaces_cfg.interval.unwrap_or(500);
glib::timeout_add_local(Duration::from_millis(interval_ms), move || {
    renderer_clone.refresh();
    glib::ControlFlow::Continue
});
```

Change `500` to `100`. That's the only change needed.

**Effect:** The workspace label updates within 100 ms of a workspace switch.
Human perception threshold for UI responsiveness is ~100–150 ms. At 100 ms
polling, workspace switches feel instant in practice.

**Cost:** `read_workspaces()` calls `gdk::property_get()` three times per tick
(one per EWMH atom). These are synchronous X11 roundtrips over a local socket —
typically 0.1–0.5 ms each, so ~1–2 ms total CPU time per 100 ms tick. CPU
impact is negligible. No subprocess spawning.

**Pros:** One-line change. Zero risk. Immediately shippable.
**Cons:** Still polling. Not event-driven. CPU cost is tiny but non-zero.

**Verdict: Ship this now** as the immediate improvement while Option A is
planned.

---

### Option D — `x11rb` crate polling the X11 display in a background thread

Use the pure-Rust `x11rb` crate to open a separate X11 connection, subscribe to
`PropertyNotify` on the root window, and push events through `mpsc`. This avoids
unsafe but adds `x11rb` as a new dependency and creates a second X11 connection.

**Verdict:** Over-engineered for this use case. Two X11 connections from the
same process is unusual and complicates authentication (MIT-MAGIC-COOKIE).
Rejected.

---

## Recommendation

1. **Ship Option C now:** Change `unwrap_or(500)` to `unwrap_or(100)` in
   `main.rs`. One-line change, shippable immediately, closes the "500ms lag"
   complaint.

2. **Plan Option A as a follow-on feature:** Implement `gdk_window_add_filter`
   unsafe FFI integration. Requirements:
   - Add `x11` to direct deps in `frames_bar/Cargo.toml`
   - Implement `WorkspacesEventFilter` struct with `add_filter`/`remove_filter`
     lifecycle methods
   - Add `SAFETY:` comments for all three `unsafe` blocks (`add_filter`,
     `remove_filter`, callback body)
   - Move `glib::timeout_add_local` self-poll out and replace with the filter
   - Add integration test (if X11 display available in test environment)

   Document clearly that this is a `frames_bar`-only feature (display code). It
   does not touch `frames_core`.

3. **Do not implement Option B** (fd-add racing with GDK's own fd reader).

---

## Standards Conflict / Proposed Update

`BAR_DESIGN.md §5` (Widget update lifecycle) currently only describes the
Poller + `glib::timeout_add_local` model. When Option A is implemented, a note
should be added:

> **Event-driven widgets:** A bar-side widget renderer may register a
> `gdk_window_add_filter` callback instead of (or in addition to) a polling
> timer, provided: (a) it is implemented only in `frames_bar`, (b) all
> `unsafe` blocks carry `SAFETY:` justifications, (c) filter teardown
> (`gdk_window_remove_filter`) is called before widget drop.

---

## Sources

- `https://docs.rs/gdk/0.18.0/gdk/struct.Window.html`: `set_events(EventMask)`,
  `EventProperty` struct — confirmed PROPERTY_CHANGE_MASK available
- `https://docs.rs/gdk/0.18.0/gdk/prelude/trait.WindowExtManual.html`: confirmed
  `add_filter` is **not** present in gdk-rs 0.18 safe Rust API
- GDK 3 C API reference: `gdk_window_add_filter`, `GdkFilterFunc`, `GdkFilterReturn`
- `crates/frames_bar/src/widgets/workspaces.rs`: current self-polling implementation
- `crates/frames_bar/src/main.rs` (~line 628): workspaces timer setup
- `crates/frames_bar/src/launcher.rs`: existing `unsafe` FFI precedent in `frames_bar`
- `standards/CODING_STANDARDS.md §8.2`: `unsafe` block documentation requirements

## Open Questions

1. **`x11` crate version:** Is `x11` crate already in `Cargo.lock` as a
   transitive dep from `gdk-sys`? If so, adding it to `frames_bar/Cargo.toml`
   as a direct dep only documents what is already linked. Confirm with
   `cargo tree -p frames_bar | grep x11` before adding.

2. **`remove_filter` on teardown:** GTK3 bar teardown sequence needs to call
   `gdk_window_remove_filter` before destroy. The `Bar` struct's `Drop` impl
   (or a teardown method) should own the filter removal. Currently `Bar` has no
   explicit `Drop` — this would be its first entry.

3. **Atom lookup timing:** EWMH atoms (`_NET_CURRENT_DESKTOP` etc.) must be
   interned after `gtk::init()` (GDK must be initialized). Confirm correct
   initialization order in `main.rs` before calling `gdk::Atom::intern`.
