use spotuify_spotify::auth::{missing_required_scopes, token_needs_scope_reauth, StoredToken};

fn token_with_scope(scope: &str) -> StoredToken {
    StoredToken {
        access_token: "access".to_string(),
        refresh_token: "refresh".to_string(),
        expires_at: 1_000,
        scope: scope.to_string(),
        token_type: "Bearer".to_string(),
    }
}

#[test]
fn token_scope_check_reports_follow_scopes_missing_from_existing_login() {
    let token = token_with_scope(
        "user-read-playback-state user-read-currently-playing user-read-recently-played \
         user-read-playback-position \
         user-modify-playback-state user-read-private playlist-read-private \
         playlist-read-collaborative playlist-modify-private playlist-modify-public \
         user-library-read user-library-modify streaming app-remote-control",
    );

    assert_eq!(
        missing_required_scopes(&token),
        vec!["user-follow-read", "user-follow-modify"]
    );
}

#[test]
fn token_scope_check_accepts_token_with_all_required_scopes() {
    let token = token_with_scope(
        "user-read-playback-state user-read-currently-playing user-read-recently-played \
         user-read-playback-position \
         user-modify-playback-state user-read-private playlist-read-private \
         playlist-read-collaborative playlist-modify-private playlist-modify-public \
         user-library-read user-library-modify user-follow-read user-follow-modify \
         streaming app-remote-control",
    );

    assert!(missing_required_scopes(&token).is_empty());
}

#[test]
fn token_needs_scope_reauth_returns_true_when_any_required_scope_is_missing() {
    // Reproduces the user-reported case: token issued before
    // follow-read / follow-modify were added to the required set.
    let token = token_with_scope(
        "user-read-playback-state user-modify-playback-state user-read-private \
         playlist-read-private playlist-modify-private user-library-read",
    );

    assert!(token_needs_scope_reauth(Some(&token)));
}

#[test]
fn token_needs_scope_reauth_returns_false_when_token_carries_every_required_scope() {
    let token = token_with_scope(
        "user-read-playback-state user-read-currently-playing user-read-recently-played \
         user-read-playback-position \
         user-modify-playback-state user-read-private playlist-read-private \
         playlist-read-collaborative playlist-modify-private playlist-modify-public \
         user-library-read user-library-modify user-follow-read user-follow-modify \
         streaming app-remote-control",
    );

    assert!(!token_needs_scope_reauth(Some(&token)));
}

#[test]
fn token_needs_scope_reauth_returns_false_when_no_token_is_stored() {
    // First-run path: no stored token means the user isn't logged in
    // yet. The right surface there is the login prompt, not a re-auth
    // banner.
    assert!(!token_needs_scope_reauth(None));
}
