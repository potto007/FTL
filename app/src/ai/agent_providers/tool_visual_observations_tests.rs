use super::*;

fn observation() -> user_context::UserBinary {
    user_context::UserBinary {
        name: "test".into(),
        content_type: "image/png".into(),
        data: "aGVsbG8=".into(),
    }
}

fn tool_response(id: &str) -> ChatMessage {
    ToolResponse::new(id.to_owned(), "{}".to_owned()).into()
}

#[test]
fn tool_images_follow_complete_result_group() {
    let mut messages = vec![
        tool_response("a"),
        tool_response("b"),
        ChatMessage::user("continue"),
    ];
    let images = HashMap::from([
        ("a".into(), vec![observation()]),
        ("b".into(), vec![observation()]),
    ]);
    append_tool_visual_observations(
        &mut messages,
        images,
        attachment_caps::AttachmentCaps {
            images: true,
            ..Default::default()
        },
        true,
    );
    assert_eq!(messages.len(), 5);
    assert!(matches!(messages[0].role, ChatRole::Tool));
    assert!(matches!(messages[1].role, ChatRole::Tool));
    assert!(messages[2]
        .content
        .iter()
        .any(|part| matches!(part, ContentPart::Binary(_))));
    assert!(messages[3]
        .content
        .iter()
        .any(|part| matches!(part, ContentPart::Binary(_))));
    assert!(messages[2]
        .content
        .first_text()
        .unwrap()
        .contains("tool call a"));
}

#[test]
fn tool_images_are_not_sent_to_text_only_models() {
    let mut messages = vec![tool_response("a")];
    append_tool_visual_observations(
        &mut messages,
        HashMap::from([("a".into(), vec![observation()])]),
        Default::default(),
        true,
    );
    assert_eq!(messages.len(), 2);
    assert!(!messages[1]
        .content
        .iter()
        .any(|part| matches!(part, ContentPart::Binary(_))));
    assert!(messages[1]
        .content
        .texts()
        .iter()
        .any(|text| text.contains("ERROR")));
}

#[test]
fn tool_images_budget_prefers_recent_observations_and_ignores_orphans() {
    let mut messages: Vec<_> = (0..10).map(|id| tool_response(&id.to_string())).collect();
    let images = (0..11)
        .map(|id| (id.to_string(), vec![observation()]))
        .collect();
    append_tool_visual_observations(
        &mut messages,
        images,
        attachment_caps::AttachmentCaps {
            images: true,
            ..Default::default()
        },
        true,
    );
    assert_eq!(messages.len(), 18);
    let observations = &messages[10..];
    assert!(observations[0]
        .content
        .first_text()
        .unwrap()
        .contains("tool call 2."));
    assert!(observations[7]
        .content
        .first_text()
        .unwrap()
        .contains("tool call 9."));
}

#[test]
fn diagnostic_snippets_do_not_contain_payloads() {
    assert!(!snippet_for_log("secret screenshot base64").contains("secret"));
    assert_eq!(snippet_for_log("secret"), "[redacted: 6 bytes]");
}

#[test]
fn tool_images_require_destination_permission() {
    let mut messages = vec![tool_response("a")];
    append_tool_visual_observations(
        &mut messages,
        HashMap::from([("a".into(), vec![observation()])]),
        attachment_caps::AttachmentCaps {
            images: true,
            ..Default::default()
        },
        false,
    );
    assert_eq!(messages.len(), 2);
    assert!(!messages[1]
        .content
        .iter()
        .any(|part| matches!(part, ContentPart::Binary(_))));
    assert!(messages[1]
        .content
        .first_text()
        .unwrap()
        .contains("withheld"));
}

#[test]
fn tool_image_memory_is_bounded_before_projection() {
    let mut images = std::collections::VecDeque::new();
    for id in 0..4 {
        remember_tool_images(&mut images, id.to_string(), vec![observation(); 3]);
    }
    assert_eq!(images.len(), 2);
    assert_eq!(images.front().unwrap().0, "2");
    remember_tool_images(&mut images, "2".into(), vec![observation()]);
    assert_eq!(
        images.iter().map(|(_, images)| images.len()).sum::<usize>(),
        4
    );
    assert_eq!(images.back().unwrap().0, "2");
}
