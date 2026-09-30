use super::{ScreenshotRegion, Vector2I};

#[test]
fn signed_regions_support_monitors_left_and_above_primary() {
    let region = ScreenshotRegion {
        top_left: Vector2I::new(-1920, -1080),
        bottom_right: Vector2I::new(0, 0),
    };
    assert!(region.validate_signed().is_ok());
    assert!(region.validate().is_err());
}

#[test]
fn signed_regions_reject_overflow_and_empty_bounds() {
    for (left, right) in [(i32::MIN, i32::MAX), (0, 0), (10, 0)] {
        let region = ScreenshotRegion {
            top_left: Vector2I::new(left, 0),
            bottom_right: Vector2I::new(right, 100),
        };
        assert!(region.validate_signed().is_err());
    }
}
