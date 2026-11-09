use semver::Version;
use serde::Deserialize;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

type ReleaseResult = Result<Option<(Version, String)>, String>;

const RELEASES_URL: &str = "https://api.github.com/repos/phillui-37/dw1-silicon/releases?per_page=100";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    html_url: String,
}

fn version_from_tag(tag: &str) -> Option<Version> {
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    let tag = tag.strip_suffix("-windows-linux").unwrap_or(tag);
    Version::parse(tag).ok()
}

fn newer_release(
    json: &str,
    current: &Version,
) -> Result<Option<(Version, String)>, serde_json::Error> {
    let releases: Vec<Release> = serde_json::from_str(json)?;
    Ok(releases
        .into_iter()
        .filter_map(|release| version_from_tag(&release.tag_name).map(|version| (release, version)))
        .filter(|(release, version)| {
            version.pre.is_empty() && !release.draft && !release.prerelease
        })
        .map(|(release, version)| (version, release.html_url))
        .filter(|(version, _)| version.cmp_precedence(current).is_gt())
        .max_by(|a, b| a.0.cmp_precedence(&b.0)))
}

fn fetch_latest(url: &str, current: &Version) -> Result<Option<(Version, String)>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(4))
        .user_agent(concat!("dw1-silicon/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client.get(url).send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("GitHub returned {}", response.status()));
    }
    newer_release(&response.text().map_err(|error| error.to_string())?, current)
        .map_err(|error| error.to_string())
}

pub(crate) struct UpdateCheck(Receiver<ReleaseResult>);
pub(crate) enum CheckPoll {
    Pending,
    Complete(Option<ReleaseResult>),
}

impl UpdateCheck {
    pub fn poll(&self) -> CheckPoll {
        match self.0.try_recv() {
            Ok(result) => CheckPoll::Complete(Some(result)),
            Err(TryRecvError::Empty) => CheckPoll::Pending,
            Err(TryRecvError::Disconnected) => CheckPoll::Complete(None),
        }
    }
}

/// Poll without waiting, clearing the receiver after either a result or a disconnect.
pub fn poll_once(check: &mut Option<UpdateCheck>) -> CheckPoll {
    let Some(active) = check.as_ref() else {
        return CheckPoll::Pending;
    };
    match active.poll() {
        CheckPoll::Pending => CheckPoll::Pending,
        CheckPoll::Complete(result) => {
            *check = None;
            CheckPoll::Complete(result)
        }
    }
}

pub fn start() -> UpdateCheck {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let current = Version::parse(env!("CARGO_PKG_VERSION")).expect("package version is semver");
        let _ = tx.send(fetch_latest(RELEASES_URL, &current));
    });
    UpdateCheck(rx)
}

fn should_open_release(button_id: i32) -> bool {
    button_id == 1
}

/// Prompt on the SDL main thread, parented to the game window.
pub fn prompt(result: Option<(Version, String)>, window: &sdl3::video::Window) {
    let Some((version, url)) = result else {
        return;
    };
    let text = format!("Version {version} is available. Open the release page in your browser?");
    let buttons = [
        sdl3::messagebox::ButtonData {
            button_id: 1,
            flags: sdl3::messagebox::MessageBoxButtonFlag::RETURNKEY_DEFAULT,
            text: "Open release page",
        },
        sdl3::messagebox::ButtonData {
            button_id: 0,
            flags: sdl3::messagebox::MessageBoxButtonFlag::ESCAPEKEY_DEFAULT,
            text: "Later",
        },
    ];
    match sdl3::messagebox::show_message_box(
        sdl3::messagebox::MessageBoxFlag::INFORMATION,
        &buttons,
        "Update available",
        &text,
        Some(window),
        None,
    ) {
        Ok(sdl3::messagebox::ClickedButton::CustomButton(button))
            if should_open_release(button.button_id) =>
        {
            let c_url = std::ffi::CString::new(url).expect("GitHub release URL has no NUL");
            if !unsafe { sdl3::sys::misc::SDL_OpenURL(c_url.as_ptr()) } {
                eprintln!("dw1: could not open release page");
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("dw1: could not show update prompt: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn browser_opens_only_for_explicit_acceptance() {
        assert!(should_open_release(1));
        assert!(!should_open_release(0));
        assert!(!should_open_release(-1));
    }

    #[test]
    fn selects_highest_stable_semver_and_ignores_drafts_prereleases_and_old_tags() {
        let data = r#"[
            {"tag_name":"v0.1.0-windows-linux","draft":false,"prerelease":false,"html_url":"old"},
            {"tag_name":"v0.2.0-rc.1","draft":false,"prerelease":false,"html_url":"rc"},
            {"tag_name":"v9.0.0","draft":true,"prerelease":false,"html_url":"draft"},
            {"tag_name":"v0.1.10","draft":false,"prerelease":false,"html_url":"ten"},
            {"tag_name":"v0.1.2","draft":false,"prerelease":false,"html_url":"two"}
        ]"#;
        let current = Version::parse("0.1.0").unwrap();
        assert_eq!(
            newer_release(data, &current).unwrap(),
            Some((Version::parse("0.1.10").unwrap(), "ten".into()))
        );
        assert_eq!(version_from_tag("v0.1.0-windows-linux"), Some(current.clone()));
        let legacy = r#"[{"tag_name":"v0.1.0-windows-linux","draft":false,"prerelease":false,"html_url":"legacy"}]"#;
        assert!(newer_release(legacy, &current).unwrap().is_none());
        assert!(newer_release(data, &Version::parse("0.1.10").unwrap())
            .unwrap()
            .is_none());
    }

    #[test]
    fn build_metadata_does_not_make_equal_precedence_a_newer_release() {
        let data = r#"[{"tag_name":"v0.1.0+other-build","draft":false,"prerelease":false,"html_url":"same"}]"#;
        let current = Version::parse("0.1.0+current-build").unwrap();
        assert!(newer_release(data, &current).unwrap().is_none());
    }

    #[test]
    fn pending_receiver_returns_immediately_and_late_result_is_consumed_once() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut check = Some(UpdateCheck(receiver));
        assert!(matches!(poll_once(&mut check), CheckPoll::Pending));
        sender
            .send(Ok(Some((Version::parse("0.1.1").unwrap(), "release-url".into()))))
            .unwrap();
        assert!(matches!(
            poll_once(&mut check),
            CheckPoll::Complete(Some(Ok(Some((version, url)))))
                if version == Version::parse("0.1.1").unwrap() && url == "release-url"
        ));
        assert!(check.is_none());
        assert!(matches!(poll_once(&mut check), CheckPoll::Pending));
    }

    #[test]
    fn disconnected_receiver_ends_check() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(sender);
        let mut check = Some(UpdateCheck(receiver));
        assert!(matches!(poll_once(&mut check), CheckPoll::Complete(None)));
        assert!(check.is_none());
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(newer_release("not json", &Version::parse("0.1.0").unwrap()).is_err());
    }

    #[test]
    fn rejects_non_success_status() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let _ = stream.read(&mut request).unwrap();
            let body = "unavailable";
            write!(stream, "HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let result = fetch_latest(
            &format!("http://{addr}/releases"),
            &Version::parse("0.1.0").unwrap(),
        );
        server.join().unwrap();
        assert!(result.unwrap_err().contains("503"));
    }

    #[test]
    fn offline_connection_returns_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        assert!(fetch_latest(
            &format!("http://{addr}/releases"),
            &Version::parse("0.1.0").unwrap(),
        )
        .is_err());
    }

    #[test]
    fn fetches_from_mock_http_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 2048];
            let n = stream.read(&mut request).unwrap();
            assert!(String::from_utf8_lossy(&request[..n]).contains("GET /releases HTTP/1.1"));
            let body = r#"[{"tag_name":"v0.1.1","draft":false,"prerelease":false,"html_url":"https://github.com/phillui-37/dw1-silicon/releases/tag/v0.1.1"}]"#;
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        });
        let result = fetch_latest(
            &format!("http://{addr}/releases"),
            &Version::parse("0.1.0").unwrap(),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(result.unwrap().0, Version::parse("0.1.1").unwrap());
    }
}
