use squarez::ui_state::{Panel, UiState};

#[test]
fn panels_start_visible_and_expanded() {
    let state = UiState::default();
    assert!(state.is_visible(Panel::Color));
    assert!(!state.is_collapsed(Panel::Color));
}

#[test]
fn toggling_visibility_hides_and_restores_panel() {
    let mut state = UiState::default();
    state.toggle_visible(Panel::Palette);
    assert!(!state.is_visible(Panel::Palette));
    state.toggle_visible(Panel::Palette);
    assert!(state.is_visible(Panel::Palette));
}

#[test]
fn toggling_collapsed_changes_only_collapse_state() {
    let mut state = UiState::default();
    state.toggle_collapsed(Panel::Layers);
    assert!(state.is_visible(Panel::Layers));
    assert!(state.is_collapsed(Panel::Layers));
}

#[test]
fn timeline_is_hideable_but_not_collapsible() {
    let mut state = UiState::default();
    state.toggle_collapsed(Panel::Timeline);
    assert!(!state.is_collapsed(Panel::Timeline));

    state.toggle_visible(Panel::Timeline);
    assert!(!state.is_visible(Panel::Timeline));
}
