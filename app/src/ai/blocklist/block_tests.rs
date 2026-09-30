use super::{CollapsibleElementState, CollapsibleExpansionState};
use crate::settings::AISettings;
use crate::test_util::settings::initialize_settings_for_tests;
use settings::Setting;
use warpui::{App, SingletonEntity};

#[test]
fn reasoning_auto_collapses_when_user_has_not_manually_toggled() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
    });
}

#[test]
fn always_show_thinking_stays_expanded_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .thinking_display_mode
                .set_value(crate::settings::ThinkingDisplayMode::AlwaysShow, ctx)
                .unwrap();
        });

        let mut state = CollapsibleElementState::default();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                scroll_pinned_to_bottom: false
            }
        ));
    });
}

#[test]
fn manual_collapse_while_streaming_stays_collapsed_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();

        state.toggle_expansion();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
    });
}

#[test]
fn manual_reexpand_while_streaming_stays_expanded_after_finish() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::default();

        state.toggle_expansion();
        state.toggle_expansion();
        app.update(|ctx| {
            state.finish_reasoning(ctx);
        });

        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                scroll_pinned_to_bottom: false
            }
        ));
    });
}

#[test]
fn start_collapsed_keeps_reasoning_available_without_opening_it() {
    use crate::settings::ThinkingDisplayMode;
    assert!(ThinkingDisplayMode::StartCollapsed.should_render());
    assert!(!ThinkingDisplayMode::NeverShow.should_render());
    let mut state = CollapsibleElementState::for_reasoning(ThinkingDisplayMode::StartCollapsed);
    state.sync_finished_state(false);
    assert!(matches!(
        state.expansion_state,
        CollapsibleExpansionState::Collapsed
    ));
}

#[test]
fn start_collapsed_stays_closed_after_completion_and_can_be_opened() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::for_reasoning(
            crate::settings::ThinkingDisplayMode::StartCollapsed,
        );
        app.update(|ctx| state.finish_reasoning(ctx));
        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
        state.toggle_expansion();
        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                ..
            }
        ));
    });
}

#[test]
fn start_collapsed_manual_expansion_survives_stream_updates_and_completion() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::for_reasoning(
            crate::settings::ThinkingDisplayMode::StartCollapsed,
        );
        state.toggle_expansion();
        state.sync_finished_state(false);
        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: false,
                ..
            }
        ));
        app.update(|ctx| state.finish_reasoning(ctx));
        app.update(|ctx| state.finish_reasoning(ctx));
        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded {
                is_finished: true,
                scroll_pinned_to_bottom: false
            }
        ));
    });
}

#[test]
fn start_collapsed_manual_recollapse_survives_completion() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        let mut state = CollapsibleElementState::for_reasoning(
            crate::settings::ThinkingDisplayMode::StartCollapsed,
        );
        state.toggle_expansion();
        state.toggle_expansion();
        app.update(|ctx| state.finish_reasoning(ctx));
        assert!(matches!(
            state.expansion_state,
            CollapsibleExpansionState::Collapsed
        ));
    });
}

#[test]
fn thinking_display_modes_preserve_defaults_and_persisted_values() {
    use crate::settings::ThinkingDisplayMode;
    assert_eq!(
        ThinkingDisplayMode::default(),
        ThinkingDisplayMode::ShowAndCollapse
    );
    for (value, mode) in [
        ("ShowAndCollapse", ThinkingDisplayMode::ShowAndCollapse),
        ("AlwaysShow", ThinkingDisplayMode::AlwaysShow),
        ("NeverShow", ThinkingDisplayMode::NeverShow),
        ("StartCollapsed", ThinkingDisplayMode::StartCollapsed),
    ] {
        let encoded = format!("\"{value}\"");
        assert_eq!(serde_json::to_string(&mode).unwrap(), encoded);
        assert_eq!(
            serde_json::from_str::<ThinkingDisplayMode>(&encoded).unwrap(),
            mode
        );
    }
}
