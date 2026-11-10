//! View action presentation; execution guards remain with each plugin.
use openidea_plugin::views::View;

/// Do not present actions whose producer says they are unavailable.
/// Temporary transport busy states are applied by the renderer afterwards.
pub fn available(mut view: View) -> View {
    view.toolbar.retain(|button| button.enabled);
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use openidea_plugin::views::{Body, Button};

    #[test]
    fn unavailable_actions_are_omitted_without_changing_content_or_order() {
        let view = View::new(Body::Message("Selection context".into()))
            .button(Button::new("create", "Create"))
            .button(Button::new("delete", "Delete").enabled(false))
            .button(Button::new("preferences", "Preferences"))
            .status("Ready");
        let filtered = available(view);
        assert_eq!(filtered.toolbar.iter().map(|button| button.id.as_str()).collect::<Vec<_>>(), ["create", "preferences"]);
        assert_eq!(filtered.body, Body::Message("Selection context".into()));
        assert_eq!(filtered.status.as_deref(), Some("Ready"));
    }
}
