//! Downloads. Everything the launcher fetches goes through here: HTTPS only, a size cap on every response, redirects
//! limited, and files verified against a SHA-256 while they are written and moved into place only when they match.

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

const USER_AGENT: &str = concat!("SKradle/", env!("CARGO_PKG_VERSION"));
const CHUNK: usize = 64 * 1024;

#[derive(Debug)]
pub enum NetError {
    /// Only `https://` URLs are fetched.
    InsecureUrl(String),
    /// The request failed (connection, TLS, timeout, too many redirects).
    Request(String),
    /// The server answered with a status other than success.
    Status(u16),
    /// The response is bigger than the cap.
    TooLarge {
        limit: u64,
    },
    /// The downloaded bytes do not have the expected SHA-256.
    Checksum {
        expected: String,
        actual: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::InsecureUrl(url) => write!(f, "refusing a non-HTTPS URL: {url}"),
            NetError::Request(why) => write!(f, "request failed: {why}"),
            NetError::Status(code) => write!(f, "the server answered {code}"),
            NetError::TooLarge { limit } => write!(f, "the response is larger than {limit} bytes"),
            NetError::Checksum { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
            NetError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NetError {}

impl From<io::Error> for NetError {
    fn from(e: io::Error) -> Self {
        NetError::Io(e)
    }
}

/// How far a download has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    /// The size announced by the server, if it announced one.
    pub total: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// The expected SHA-256 as hex (any case). The file is only kept when it matches.
    pub sha256: Option<String>,
    /// The most bytes accepted.
    pub max_bytes: u64,
}

impl DownloadOptions {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            sha256: None,
            max_bytes,
        }
    }

    pub fn with_sha256(mut self, hex: impl Into<String>) -> Self {
        self.sha256 = Some(hex.into());
        self
    }
}

#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    https_only: bool,
    /// The whole-request limit for small documents (a status, a listing, a report): a stalled connection must not
    /// hold them up for as long as a big download is allowed to take.
    document_timeout: Duration,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self::build(true)
    }

    /// A client that also accepts plain `http://`, for tests that talk to a loopback server.
    #[cfg(test)]
    pub(crate) fn plain_http_for_tests() -> Self {
        Self::build(false)
    }

    /// A client with short timeouts, for requests that must not hold anything up (crash reports sent from a panic
    /// hook, status pings). Everything else uses [`Client::new`], which allows a long download.
    pub fn with_timeouts(connect: Duration, total: Duration) -> Self {
        Self::build_with(true, connect, total)
    }

    fn build(https_only: bool) -> Self {
        Self::build_with(
            https_only,
            Duration::from_secs(15),
            Duration::from_secs(30 * 60),
        )
    }

    fn build_with(https_only: bool, connect: Duration, total: Duration) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .https_only(https_only)
            .max_redirects(4)
            .timeout_connect(Some(connect))
            .timeout_recv_response(Some(total.min(Duration::from_secs(60))))
            .timeout_global(Some(total))
            .user_agent(USER_AGENT)
            .build()
            .into();
        Self {
            agent,
            https_only,
            document_timeout: total.min(Duration::from_secs(120)),
        }
    }

    fn check_url(&self, url: &str) -> Result<(), NetError> {
        if self.https_only && !url.starts_with("https://") {
            return Err(NetError::InsecureUrl(url.to_owned()));
        }
        Ok(())
    }

    fn open(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, NetError> {
        self.check_url(url)?;
        self.agent.get(url).call().map_err(|e| match e {
            ureq::Error::StatusCode(code) => NetError::Status(code),
            other => NetError::Request(other.to_string()),
        })
    }

    /// Like `open`, with the shorter limit for small documents.
    fn open_document(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, NetError> {
        self.check_url(url)?;
        self.agent
            .get(url)
            .config()
            .timeout_global(Some(self.document_timeout))
            .build()
            .call()
            .map_err(|e| match e {
                ureq::Error::StatusCode(code) => NetError::Status(code),
                other => NetError::Request(other.to_string()),
            })
    }

    /// Fetches a small document into memory.
    pub fn get_bytes(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, NetError> {
        let mut response = self.open_document(url)?;
        let mut out = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(max_bytes.saturating_add(1))
            .read_to_end(&mut out)?;
        if out.len() as u64 > max_bytes {
            return Err(NetError::TooLarge { limit: max_bytes });
        }
        Ok(out)
    }

    pub fn get_text(&self, url: &str, max_bytes: u64) -> Result<String, NetError> {
        let bytes = self.get_bytes(url, max_bytes)?;
        String::from_utf8(bytes)
            .map_err(|e| NetError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
    }

    /// Sends `body` with a POST and returns the (size-capped) response body. HTTPS only like everything else.
    pub fn post(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        content_type: &str,
        body: &[u8],
        max_response: u64,
    ) -> Result<Vec<u8>, NetError> {
        self.check_url(url)?;
        let mut request = self
            .agent
            .post(url)
            .config()
            .timeout_global(Some(self.document_timeout))
            .build()
            .content_type(content_type);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request.send(body).map_err(|e| match e {
            ureq::Error::StatusCode(code) => NetError::Status(code),
            other => NetError::Request(other.to_string()),
        })?;
        let mut out = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(max_response.saturating_add(1))
            .read_to_end(&mut out)?;
        if out.len() as u64 > max_response {
            return Err(NetError::TooLarge {
                limit: max_response,
            });
        }
        Ok(out)
    }

    /// Downloads `url` to `dest`. The bytes go to `<dest>.part` first; `dest` appears only when the size cap held and
    /// the checksum (if given) matched, so a failed or interrupted download never leaves a believable file behind.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        options: &DownloadOptions,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<u64, NetError> {
        let mut response = self.open(url)?;
        let total = response.body().content_length();
        if total.is_some_and(|t| t > options.max_bytes) {
            return Err(NetError::TooLarge {
                limit: options.max_bytes,
            });
        }
        let part = part_path(dest);
        let result = (|| {
            if let Some(folder) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(folder)?;
            }
            // Whatever is at the temporary path (a stale file, a link planted there) is removed, never written
            // through; the new file must not exist yet.
            match std::fs::remove_file(&part) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&part)?;
            let mut hasher = Sha256::new();
            let mut reader = response.body_mut().as_reader();
            let mut buf = vec![0u8; CHUNK];
            let mut done = 0u64;
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                done += n as u64;
                if done > options.max_bytes {
                    return Err(NetError::TooLarge {
                        limit: options.max_bytes,
                    });
                }
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n])?;
                progress(Progress { done, total });
            }
            file.sync_all()?;
            drop(file);
            if let Some(expected) = &options.sha256 {
                let actual = hex(hasher.finalize().as_slice());
                if !actual.eq_ignore_ascii_case(expected.trim()) {
                    return Err(NetError::Checksum {
                        expected: expected.trim().to_ascii_lowercase(),
                        actual,
                    });
                }
            }
            std::fs::rename(&part, dest)?;
            Ok(done)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

/// Lower-case hex of `bytes`.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).as_slice())
}

/// SHA-256 of a file, read in chunks.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(hasher.finalize().as_slice()));
        }
        hasher.update(&buf[..n]);
    }
}

/// A short TCP probe of a game server: resolves `host`, tries each address (at most four) in turn and returns how
/// long the connection took, or `None` when none accepted within `timeout` (counted over all addresses). Nothing is
/// sent: the connection is closed at once. Name resolution has no timeout of its own, so call this from a worker
/// thread, never from the UI.
pub fn probe_tcp(host: &str, port: u16, timeout: Duration) -> Option<Duration> {
    let host = host.trim().trim_matches(['[', ']']);
    let deadline = Instant::now() + timeout;
    for addr in (host, port).to_socket_addrs().ok()?.take(4) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let started = Instant::now();
        if TcpStream::connect_timeout(&addr, left).is_ok() {
            return Some(started.elapsed());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use std::thread;

    /// Serves `routes` (path to status and body) on a loopback port for the rest of the test.
    fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = io::BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let path = request_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_owned();
                let (status, body) = routes
                    .iter()
                    .find(|(p, _, _)| *p == path)
                    .map(|(_, s, b)| (*s, b.clone()))
                    .unwrap_or((404, Vec::new()));
                let mut stream = stream;
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{addr}")
    }

    fn test_client() -> Client {
        Client::build(false)
    }

    #[test]
    fn hashes_match_known_values() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn plain_http_is_refused_by_default() {
        let err = Client::new()
            .get_bytes("http://example.invalid/x", 10)
            .unwrap_err();
        assert!(matches!(err, NetError::InsecureUrl(_)), "{err}");
        let dir = tempfile::tempdir().unwrap();
        let err = Client::new()
            .download(
                "http://example.invalid/x",
                &dir.path().join("f"),
                &DownloadOptions::new(10),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, NetError::InsecureUrl(_)));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn fetches_small_documents_and_enforces_the_cap() {
        let base = serve(vec![("/doc", 200, b"hello".to_vec())]);
        let client = test_client();
        assert_eq!(
            client.get_text(&format!("{base}/doc"), 100).unwrap(),
            "hello"
        );
        let err = client.get_bytes(&format!("{base}/doc"), 3).unwrap_err();
        assert!(matches!(err, NetError::TooLarge { limit: 3 }), "{err}");
    }

    #[test]
    fn reports_error_statuses() {
        let base = serve(vec![]);
        let err = test_client()
            .get_bytes(&format!("{base}/nope"), 10)
            .unwrap_err();
        assert!(matches!(err, NetError::Status(404)), "{err}");
    }

    #[test]
    fn downloads_verifies_and_reports_progress() {
        let body = vec![7u8; 200_000];
        let base = serve(vec![("/file", 200, body.clone())]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let options = DownloadOptions::new(1_000_000).with_sha256(sha256_hex(&body).to_uppercase());
        let mut last = None;
        let n = test_client()
            .download(&format!("{base}/file"), &dest, &options, &mut |p| {
                last = Some(p)
            })
            .unwrap();
        assert_eq!(n, 200_000);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(
            last,
            Some(Progress {
                done: 200_000,
                total: Some(200_000)
            })
        );
        assert_eq!(sha256_file(&dest).unwrap(), sha256_hex(&body));
        assert!(!dir.path().join("file.bin.part").exists());
    }

    #[test]
    fn a_wrong_checksum_leaves_nothing_behind() {
        let base = serve(vec![("/file", 200, b"payload".to_vec())]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("file.bin");
        let options = DownloadOptions::new(100).with_sha256("00".repeat(32));
        let err = test_client()
            .download(&format!("{base}/file"), &dest, &options, &mut |_| {})
            .unwrap_err();
        assert!(matches!(err, NetError::Checksum { .. }), "{err}");
        assert!(
            std::fs::read_dir(dir.path()).unwrap().next().is_none(),
            "no file and no .part"
        );
    }

    #[test]
    fn an_oversized_download_is_rejected_and_cleaned_up() {
        let base = serve(vec![("/big", 200, vec![0u8; 5000])]);
        let dir = tempfile::tempdir().unwrap();
        let err = test_client()
            .download(
                &format!("{base}/big"),
                &dir.path().join("big"),
                &DownloadOptions::new(1000),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, NetError::TooLarge { limit: 1000 }), "{err}");
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn post_sends_the_headers_and_the_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/envelope", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = io::BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
            tx.send((head, body)).unwrap();
        });
        let reply = test_client()
            .post(
                &url,
                &[("X-Test", "yes")],
                "application/x-test",
                b"payload",
                100,
            )
            .unwrap();
        assert_eq!(reply, b"ok");
        let (head, body) = rx.recv().unwrap();
        assert!(head.starts_with("POST /envelope"), "{head}");
        assert!(head.to_ascii_lowercase().contains("x-test: yes"), "{head}");
        assert!(
            head.to_ascii_lowercase()
                .contains("content-type: application/x-test"),
            "{head}"
        );
        assert_eq!(body, b"payload");
    }

    #[test]
    fn post_refuses_plain_http_by_default() {
        let err = Client::new()
            .post("http://example.invalid/x", &[], "text/plain", b"x", 10)
            .unwrap_err();
        assert!(matches!(err, NetError::InsecureUrl(_)));
    }

    #[test]
    fn a_download_creates_missing_folders() {
        let base = serve(vec![("/file", 200, b"payload".to_vec())]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a/b/file.bin");
        test_client()
            .download(
                &format!("{base}/file"),
                &dest,
                &DownloadOptions::new(100),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"payload");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_planted_at_the_temporary_path_is_replaced_not_written_through() {
        let base = serve(vec![("/file", 200, b"payload".to_vec())]);
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim.txt");
        std::fs::write(&victim, b"precious").unwrap();
        let dest = dir.path().join("file.bin");
        std::os::unix::fs::symlink(&victim, dir.path().join("file.bin.part")).unwrap();
        test_client()
            .download(
                &format!("{base}/file"),
                &dest,
                &DownloadOptions::new(100),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"payload");
        assert_eq!(std::fs::read(&victim).unwrap(), b"precious");
    }

    #[test]
    fn a_limit_of_u64_max_does_not_overflow() {
        let base = serve(vec![("/doc", 200, b"hello".to_vec())]);
        let bytes = test_client()
            .get_bytes(&format!("{base}/doc"), u64::MAX)
            .unwrap();
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn an_existing_file_survives_a_failed_download() {
        let base = serve(vec![("/file", 200, b"new".to_vec())]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("keep.bin");
        std::fs::write(&dest, b"old").unwrap();
        let options = DownloadOptions::new(100).with_sha256("11".repeat(32));
        assert!(test_client()
            .download(&format!("{base}/file"), &dest, &options, &mut |_| {})
            .is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"old");
    }

    #[test]
    fn a_server_that_never_answers_is_abandoned_within_the_short_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/slow", listener.local_addr().unwrap());
        thread::spawn(move || {
            // Accept and hold the connection open without ever replying.
            let held: Vec<_> = listener.incoming().flatten().collect();
            thread::sleep(std::time::Duration::from_secs(10));
            drop(held);
        });
        let client = Client::build_with(
            false,
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(1),
        );
        let started = std::time::Instant::now();
        let err = client.post(&url, &[], "text/plain", b"x", 10).unwrap_err();
        assert!(matches!(err, NetError::Request(_)), "{err}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(4),
            "gave up after {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_tcp_probe_times_a_listening_port_and_gives_up_on_a_closed_one() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let rtt = probe_tcp("127.0.0.1", port, Duration::from_secs(2)).expect("reachable");
        assert!(rtt < Duration::from_secs(2));
        drop(listener);
        assert_eq!(
            probe_tcp("127.0.0.1", port, Duration::from_millis(500)),
            None
        );
        assert_eq!(probe_tcp("", 80, Duration::from_millis(100)), None);
    }
}
