use wonderland_client_app::authoring::*;
use wonderland_contracts::authoring::*;

fn request(projection: &AuthoringProjection, operation: &str) -> AuthoringRequest {
    let kind = AuthoringRequestKind::CreateProfile {
        name: "Refreshed content".into(),
        description: String::new(),
        shard_id: None,
        appearance: projection.appearance_content.default_selection(),
    };
    AuthoringRequest {
        operation_id: operation.into(),
        base_revision: projection.revision,
        expected_sources: projection.source_revisions(&kind),
        kind,
    }
}

#[test]
fn projection_refresh_keeps_replay_and_rejects_reused_operation_ids() {
    let original = preview_authoring_projection();
    let mut provider = PreviewAuthoringProvider::new(original.clone()).unwrap();
    let first = request(&original, "before-content-load");
    let receipt = provider.handle(&first);
    assert!(matches!(receipt, AuthoringEvent::Committed { .. }));
    let mut refreshed = provider.snapshot().clone();
    refreshed.revision += 1;
    refreshed.appearance_content.revision += 1;
    provider.replace_projection(refreshed.clone()).unwrap();
    assert_eq!(provider.handle(&first), receipt);
    assert_eq!(provider.snapshot(), &refreshed);
    assert!(matches!(
        provider.handle(&request(&refreshed, "before-content-load")),
        AuthoringEvent::Rejected {
            error: AuthoringError::InvalidOperation,
            ..
        }
    ));
    assert!(matches!(
        provider.handle(&request(&refreshed, "after-content-load")),
        AuthoringEvent::Committed { .. }
    ));
}

#[test]
fn invalid_or_non_newer_refresh_cannot_replace_the_snapshot() {
    let original = preview_authoring_projection();
    let mut provider = PreviewAuthoringProvider::new(original.clone()).unwrap();
    assert_eq!(
        provider.replace_projection(original.clone()),
        Err(AuthoringError::StaleRevision)
    );
    let mut invalid = original.clone();
    invalid.revision += 1;
    invalid.catalog_revision = 0;
    assert!(matches!(
        provider.replace_projection(invalid),
        Err(AuthoringError::InvalidProjection(_))
    ));
    assert_eq!(provider.snapshot(), &original);
}

#[test]
fn content_refresh_does_not_reset_the_session_operation_budget() {
    let original = preview_authoring_projection();
    let mut provider = PreviewAuthoringProvider::new(original.clone()).unwrap();
    for index in 0..MAX_PREVIEW_OPERATIONS {
        let mut stale = request(&original, &format!("stale-{index}"));
        stale.base_revision = 0;
        assert!(matches!(
            provider.handle(&stale),
            AuthoringEvent::Rejected {
                error: AuthoringError::StaleRevision,
                ..
            }
        ));
    }
    let mut refreshed = original;
    refreshed.revision += 1;
    provider.replace_projection(refreshed.clone()).unwrap();
    assert!(matches!(
        provider.handle(&request(&refreshed, "after-refresh")),
        AuthoringEvent::Rejected {
            error: AuthoringError::OperationLimit,
            ..
        }
    ));
}
