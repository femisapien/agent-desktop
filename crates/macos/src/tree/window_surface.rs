use agent_desktop_core::{AdapterError, ErrorCode, Rect, SnapshotSurface, WindowInfo};
use std::time::Instant;

use super::AXElement;

pub(crate) fn surface_for_window(
    window: &WindowInfo,
    surface: SnapshotSurface,
    deadline: Instant,
) -> Result<Option<AXElement>, AdapterError> {
    resolve_owned_surface(
        crate::system::window_resolve::window_element_for_info_with_deadline(window, deadline),
        |owner| super::surfaces::surface_in_window(&owner, surface, deadline),
        || {
            let pid = crate::system::process_identity::to_pid_t(window.pid)?;
            let Some(found) = surface_with_frame(pid, surface, deadline)? else {
                return Ok(None);
            };
            Ok(window
                .bounds
                .filter(|bounds| frames_match(&found.1, bounds))
                .map(|_| found.0))
        },
    )
}

fn resolve_owned_surface<T>(
    owner: Result<T, AdapterError>,
    in_window: impl FnOnce(T) -> Result<Option<T>, AdapterError>,
    fallback: impl FnOnce() -> Result<Option<T>, AdapterError>,
) -> Result<Option<T>, AdapterError> {
    match owner {
        Ok(owner) => in_window(owner),
        Err(error)
            if error.code == ErrorCode::ActionNotSupported
                && error
                    .details
                    .as_ref()
                    .and_then(|details| details["kind"].as_str())
                    == Some("window_without_accessibility_element") =>
        {
            fallback()
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "macos")]
pub(super) fn surface_for_pid_with_bounds_hash(
    pid: i32,
    surface: SnapshotSurface,
    expected_hash: Option<u64>,
    deadline: Instant,
) -> Result<Option<AXElement>, AdapterError> {
    let Some(expected_hash) = expected_hash else {
        return Ok(None);
    };
    Ok(surface_with_frame(pid, surface, deadline)?
        .filter(|(_, bounds)| saved_window_frame_matches(bounds, Some(expected_hash)))
        .map(|(element, _)| element))
}

fn surface_with_frame(
    pid: i32,
    surface: SnapshotSurface,
    deadline: Instant,
) -> Result<Option<(AXElement, Rect)>, AdapterError> {
    let element = match surface {
        SnapshotSurface::Sheet => super::surfaces::sheet_for_pid(pid, deadline)?,
        SnapshotSurface::Popover => super::surfaces::popover_for_pid(pid, deadline)?,
        SnapshotSurface::Alert => super::surfaces::alert_for_pid(pid, deadline)?,
        _ => return Err(AdapterError::not_supported("window-owned surface")),
    };
    let Some(element) = element else {
        return Ok(None);
    };
    let bounds = super::element_bounds::read_bounds_with_deadline(&element, deadline)?;
    Ok(bounds.map(|bounds| (element, bounds)))
}

#[cfg(target_os = "macos")]
fn saved_window_frame_matches(found: &Rect, expected_hash: Option<u64>) -> bool {
    expected_hash.is_some() && found.bounds_hash() == expected_hash
}

fn frames_match(found: &Rect, window: &Rect) -> bool {
    found.validate().is_ok()
        && window.validate().is_ok()
        && (found.x - window.x).abs() <= 2.0
        && (found.y - window.y).abs() <= 2.0
        && (found.width - window.width).abs() <= 2.0
        && (found.height - window.height).abs() <= 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 200.0,
        }
    }

    #[test]
    fn frames_match_identical_bounds() {
        assert!(frames_match(&frame(), &frame()));
    }

    #[test]
    fn frames_match_within_two_points() {
        let found = Rect {
            x: 12.0,
            y: 18.0,
            width: 302.0,
            height: 198.0,
        };
        assert!(frames_match(&found, &frame()));
    }

    #[test]
    fn frames_reject_position_outside_tolerance() {
        for found in [
            Rect {
                x: 12.01,
                ..frame()
            },
            Rect {
                y: 17.99,
                ..frame()
            },
        ] {
            assert!(!frames_match(&found, &frame()));
        }
    }

    #[test]
    fn frames_reject_different_size() {
        for found in [
            Rect {
                width: 303.0,
                ..frame()
            },
            Rect {
                height: 197.0,
                ..frame()
            },
        ] {
            assert!(!frames_match(&found, &frame()));
        }
    }

    #[test]
    fn saved_window_hash_rejects_missing_or_other_window_geometry() {
        let expected = frame().bounds_hash();
        assert!(saved_window_frame_matches(&frame(), expected));
        assert!(!saved_window_frame_matches(&frame(), None));
        assert!(!saved_window_frame_matches(
            &Rect { x: 50.0, ..frame() },
            expected
        ));
    }

    #[test]
    fn missing_ax_window_uses_bounds_checked_fallback() {
        let error = AdapterError::new(ErrorCode::ActionNotSupported, "no AX window")
            .with_details(serde_json::json!({"kind": "window_without_accessibility_element"}));
        for (found, expected) in [
            (frame(), Some(frame())),
            (Rect { x: 50.0, ..frame() }, None),
        ] {
            let result = resolve_owned_surface::<Rect>(
                Err(error.clone()),
                |_| panic!("no owner"),
                || Ok(frames_match(&found, &frame()).then_some(found)),
            )
            .unwrap();
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn other_owner_errors_propagate_without_fallback() {
        for error in [
            AdapterError::timeout("owner"),
            AdapterError::new(ErrorCode::ActionNotSupported, "other"),
        ] {
            let result = resolve_owned_surface::<Rect>(
                Err(error.clone()),
                |_| panic!("no owner"),
                || panic!("must propagate"),
            );
            let actual = result.unwrap_err();
            assert_eq!(actual.code, error.code);
            assert_eq!(actual.message, error.message);
            assert_eq!(actual.details, error.details);
        }
    }
}
