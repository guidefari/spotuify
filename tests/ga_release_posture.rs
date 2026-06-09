fn repo_file(path: &str) -> std::io::Result<String> {
    std::fs::read_to_string(path)
}

#[test]
fn ga_docs_state_byo_spotify_app_scope_not_broad_consumer_auth() -> std::io::Result<()> {
    let readme = repo_file("README.md")?;
    let install = repo_file("site/src/content/docs/getting-started/install.md")?;
    let first_run = repo_file("site/src/content/docs/getting-started/first-run.md")?;

    for (name, doc) in [
        ("README.md", readme.as_str()),
        ("install.md", install.as_str()),
        ("first-run.md", first_run.as_str()),
    ] {
        assert!(
            doc.contains("BYO Spotify app GA"),
            "{name} must explicitly scope GA as BYO Spotify app GA"
        );
        assert!(
            doc.contains("not broad consumer no-developer setup"),
            "{name} must not imply broad consumer auth is already solved"
        );
        assert!(
            doc.contains("Extended Quota Mode"),
            "{name} must give users the actionable Spotify policy path for write failures"
        );
    }
    Ok(())
}

#[test]
fn release_docs_require_read_only_and_mutation_live_smokes_before_ga() -> std::io::Result<()> {
    let readme = repo_file("README.md")?;
    let conformance = repo_file("docs/implementation/07-testing-conformance.md")?;

    for (name, doc) in [
        ("README.md", readme.as_str()),
        (
            "docs/implementation/07-testing-conformance.md",
            conformance.as_str(),
        ),
    ] {
        assert!(
            doc.contains("Before calling a release GA-ready"),
            "{name} must frame the live smoke as a release gate"
        );
        assert!(
            doc.contains("SPOTUIFY_GA_LIVE_PLAYBACK=1"),
            "{name} must require the playback mutation smoke command"
        );
        assert!(
            doc.contains("SPOTUIFY_GA_LIVE_PLAYLIST=1"),
            "{name} must require the playlist mutation smoke command"
        );
    }
    Ok(())
}

#[test]
fn macos_dmg_signing_allows_homebrew_portaudio_for_bundled_cli() -> std::io::Result<()> {
    let script = repo_file("clients/macos/scripts/build-dmg.sh")?;
    let entitlements = repo_file("clients/macos/Support/spotuify-cli.entitlements")?;

    assert!(
        script.contains("--entitlements \"$cli_entitlements\""),
        "DMG signing must pass CLI entitlements when signing the bundled spotuify binary"
    );
    assert!(
        entitlements.contains("com.apple.security.cs.disable-library-validation"),
        "bundled CLI needs library validation disabled so Homebrew PortAudio can load"
    );
    Ok(())
}
