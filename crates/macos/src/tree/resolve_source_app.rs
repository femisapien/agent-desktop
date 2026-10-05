use agent_desktop_core::{AdapterError, RefEntry};

use super::{AXElement, resolve_read_context::ResolveReadContext};

#[cfg(target_os = "macos")]
pub(super) fn verify_source_application(
    application: &AXElement,
    entry: &RefEntry,
    context: &mut ResolveReadContext,
) -> Result<(), AdapterError> {
    let Some(expected) = entry
        .source
        .source_app
        .as_deref()
        .filter(|name| !name.is_empty())
    else {
        return Ok(());
    };
    let actual = super::resolve_ax_read::read_string_with_usage(
        application,
        "AXTitle",
        context.deadline,
        &mut context.usage,
    )?;
    if source_app_matches(expected, actual.as_deref(), || {
        let pid = crate::system::process_identity::to_pid_t(entry.process.pid)?;
        let inventory =
            crate::system::workspace_apps::window_owner_snapshot_until(context.deadline)?;
        Ok(inventory.owner(pid).map(|owner| owner.name.clone()))
    })? {
        return Ok(());
    }
    Err(
        AdapterError::element_not_found("source application").with_details(serde_json::json!({
            "kind": "source_process_identity",
            "pid": entry.process.pid,
            "expected_app": expected,
            "actual_app": actual,
            "complete": true,
            "retryable": false,
        })),
    )
}

fn source_app_matches(
    expected: &str,
    title: Option<&str>,
    inventory_name: impl FnOnce() -> Result<Option<String>, AdapterError>,
) -> Result<bool, AdapterError> {
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        return Ok(agent_desktop_core::app_name_matches(title, expected));
    }
    Ok(inventory_name()?.is_some_and(|name| agent_desktop_core::app_name_matches(&name, expected)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_title_uses_inventory_name() {
        for title in [None, Some(""), Some(" \t\n")] {
            assert!(source_app_matches("Fixture", title, || Ok(Some("fIXTURE".into()))).unwrap());
        }
    }

    #[test]
    fn blank_title_rejects_different_or_missing_inventory_name() {
        for name in [None, Some("Other".into())] {
            assert!(!source_app_matches("Fixture", Some(" "), || Ok(name)).unwrap());
        }
    }

    #[test]
    fn readable_title_never_reads_inventory() {
        for (title, matches) in [("fIXTURE", true), ("Other", false)] {
            assert_eq!(
                source_app_matches("Fixture", Some(title), || {
                    panic!("readable AXTitle must not read inventory")
                })
                .unwrap(),
                matches
            );
        }
    }

    #[test]
    fn inventory_read_error_propagates() {
        let error = source_app_matches("Fixture", None, || Err(AdapterError::timeout("inventory")))
            .unwrap_err();
        assert_eq!(error.code, agent_desktop_core::ErrorCode::Timeout);
        assert_eq!(error.message, "inventory");
    }
}
