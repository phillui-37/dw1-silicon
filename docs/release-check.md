# Release update check

The desktop game starts a release check on a background thread during startup. It uses GitHub's releases API with a four second request timeout; request and API errors are logged and do not block gameplay. After the splash screen, the SDL main thread retains the checker and polls it with `try_recv` during the regular event pump. Pending checks return immediately, and a result that arrives later is still consumed. Completion, error, or channel disconnect clears the checker, so a release prompt can be offered at most once. The modal prompt is parented to the game window. The browser is opened only when the user chooses **Open release page**.

Release selection compares SemVer precedence, which ignores build metadata. Drafts, GitHub prereleases, and semver prerelease tags are excluded. The legacy `v0.1.0-windows-linux` tag is normalized to `0.1.0`; it is the original release baseline and will not prompt in newer builds. Future tags should use ordinary semver such as `v0.1.1`.

Before building a new release, update the version in `crates/dw1/Cargo.toml` and refresh `Cargo.lock`. Use the same version in the release tag so installed builds can detect later releases correctly.
