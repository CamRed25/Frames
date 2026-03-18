//! Workspaces widget stub — placeholder data provider for `parapet_core`.
//!
//! This module returns static placeholder data only. The real X11 EWMH query
//! (`_NET_NUMBER_OF_DESKTOPS`, `_NET_CURRENT_DESKTOP`, `_NET_DESKTOP_NAMES`)
//! lives in the `parapet_bar` workspace renderer, which has access to GDK and
//! X11 APIs. `parapet_core` must not import any display-system types.
//!
//! This stub satisfies the [`Widget`] trait contract so the [`crate::poll::Poller`]
//! can register a workspace widget entry without special-casing. The `parapet_bar`
//! workspace renderer self-polls X11 directly and does not consume data from
//! this stub — see `DOCS/futures.md` for the architectural debt note.

use crate::error::ParapetError;
use crate::widget::{Widget, WidgetData};

/// Workspace widget stub.
///
/// Always returns a single workspace (count=1, active=0, names=[]) as
/// placeholder data. Real workspace information is queried by `parapet_bar`.
pub struct WorkspacesWidget {
    name: String,
}

impl WorkspacesWidget {
    /// Create a new `WorkspacesWidget` stub.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Widget for WorkspacesWidget {
    fn name(&self) -> &str {
        &self.name
    }

    /// Return static placeholder workspace data.
    ///
    /// Returns `count: 1, active: 0, names: []` unconditionally. Real data
    /// is provided by the `parapet_bar` workspace renderer via X11 EWMH.
    ///
    /// # Errors
    ///
    /// Always returns `Ok`.
    fn update(&mut self) -> Result<WidgetData, ParapetError> {
        Ok(WidgetData::Workspaces {
            count: 1,
            active: 0,
            names: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspaces_stub_returns_valid_data() {
        let mut w = WorkspacesWidget::new("workspaces");
        let data = w.update().expect("workspaces update should not fail");
        if let WidgetData::Workspaces {
            count,
            active,
            names,
        } = data
        {
            assert!(count >= 1, "count must be at least 1");
            assert!(active < count, "active must be a valid index");
            let _ = names; // empty is valid for the stub
        } else {
            panic!("expected WidgetData::Workspaces");
        }
    }

    #[test]
    fn workspaces_name_non_empty() {
        let w = WorkspacesWidget::new("workspaces");
        assert!(!w.name().is_empty());
    }

    #[test]
    fn workspaces_satisfies_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WorkspacesWidget>();
    }
}
