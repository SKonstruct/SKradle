//! The server this player hosts ("My server"): the interface the UI needs from the server-host module.
//! Tortodrone's own web UI is the management interface; the UI embeds it in a web view (or opens the system
//! browser) at the address given here.
//!
//! TODO(serverhost): the real implementation (start and stop the server, readiness, credentials, the
//! `http://127.0.0.1:<port>/` address) comes with the server-host module. Until then `StubHost` answers with a
//! placeholder page, so the web view path can be exercised.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // the stub is always ready; the real host uses the others
pub enum HostState {
    Stopped,
    Starting,
    Ready,
    Failed,
}

impl HostState {
    pub fn as_str(self) -> &'static str {
        match self {
            HostState::Stopped => "stopped",
            HostState::Starting => "starting",
            HostState::Ready => "ready",
            HostState::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostStatus {
    pub state: HostState,
    /// The management UI's address once the server is ready.
    pub url: Option<String>,
    /// A sentence for the player when `Failed`.
    pub message: String,
}

pub trait ServerHost: Send + Sync {
    fn status(&self) -> HostStatus;
}

/// Placeholder host: always ready, serving an inline page.
pub struct StubHost;

impl ServerHost for StubHost {
    fn status(&self) -> HostStatus {
        HostStatus {
            state: HostState::Ready,
            url: Some(data_url(STUB_PAGE)),
            message: String::new(),
        }
    }
}

const STUB_PAGE: &str = "<!doctype html><meta charset=utf-8><title>My server</title>\
<style>body{font:15px system-ui,sans-serif;margin:40px;color:#222}h1{font-size:22px}code{background:#eee;padding:2px 6px;border-radius:4px}</style>\
<h1>My server</h1><p>This is a placeholder page. The server's own management interface appears here once the \
server-host module is connected.</p><p>Embedded web view check: <code id=js>script not run</code></p>\
<script>document.getElementById('js').textContent='script ran'</script>";

/// A `data:` URL for an HTML page (percent-encoded, so it survives every URL parser).
pub fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html;charset=utf-8,");
    for byte in html.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_urls_are_fully_encoded() {
        assert_eq!(
            data_url("<a b>"),
            "data:text/html;charset=utf-8,%3Ca%20b%3E"
        );
        assert!(!data_url("é#?&").contains(['#', '?', '&', 'é']));
    }

    #[test]
    fn the_stub_is_ready_with_an_address() {
        let status = StubHost.status();
        assert_eq!(status.state, HostState::Ready);
        assert!(status.url.unwrap().starts_with("data:text/html"));
    }
}
