//! Port checks before a start, and the readiness probe for the web listener.
//!
//! A port counts as busy when it cannot be bound on loopback or on every interface, or when something answers a
//! connection on `127.0.0.1`. The three checks are all needed: on some systems a bind to the wildcard address
//! succeeds while another program holds the loopback address, and a listener that is full or paused still accepts
//! a connection. IPv6 is not checked (the engine binds an IPv4 address). A free port can still be taken between the
//! check and the start; the process manager also reads the engine's own bind failure.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};
use std::time::Duration;

use super::config::HostConfig;

/// Which listener a port is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortUse {
    Game,
    Web,
}

/// A port that cannot be used, with the next free one above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub which: PortUse,
    pub port: u16,
    /// The same number is configured for both listeners.
    pub same_as_other: bool,
    pub suggestion: Option<u16>,
}

/// True when nothing holds `port` on this machine (see the module notes).
pub fn is_free(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    let loopback = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    if TcpStream::connect_timeout(&loopback, Duration::from_millis(250)).is_ok() {
        return false;
    }
    // One after the other, each released before the next: on Linux a wildcard bind conflicts with any listener on
    // the same port, including a loopback listener of this very probe.
    let on_loopback = TcpListener::bind(loopback).is_ok();
    let on_all = TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(
        Ipv4Addr::UNSPECIFIED,
        port,
    )))
    .is_ok();
    on_loopback && on_all
}

/// The first port above `start` (up to 200 further) that `free` accepts and is not in `avoid`.
pub fn suggest_with(start: u16, avoid: &[u16], free: &dyn Fn(u16) -> bool) -> Option<u16> {
    (1..=200u32)
        .filter_map(|step| u16::try_from(u32::from(start) + step).ok())
        .find(|p| !avoid.contains(p) && free(*p))
}

pub fn suggest(start: u16, avoid: &[u16]) -> Option<u16> {
    suggest_with(start, avoid, &is_free)
}

/// Every problem with the configured ports, game first. Empty when both can be used.
pub fn check_with(config: &HostConfig, free: &dyn Fn(u16) -> bool) -> Vec<Conflict> {
    let same = config.game_port == config.web_port;
    let mut conflicts = Vec::new();
    for (which, port, other) in [
        (PortUse::Game, config.game_port, config.web_port),
        (PortUse::Web, config.web_port, config.game_port),
    ] {
        if same || !free(port) {
            conflicts.push(Conflict {
                which,
                port,
                same_as_other: same,
                suggestion: suggest_with(port, &[other], free),
            });
        }
    }
    conflicts
}

pub fn check(config: &HostConfig) -> Vec<Conflict> {
    check_with(config, &is_free)
}

/// What the web listener answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebProbe {
    /// Nothing accepted the connection, or it did not speak HTTP.
    NotListening,
    /// The listener answered with this status. `200` is the WebUI's public first-run query succeeding; `403`
    /// means the listener is up but the engine does not accept the `Host` the probe sent (a custom
    /// `webui-public-url`).
    Answers(u16),
}

impl WebProbe {
    pub fn is_ready(self) -> bool {
        self == WebProbe::Answers(200)
    }
}

/// Asks the web listener on loopback for `GET /admin/api/setup`, the one admin route that needs no login. Only
/// ever connects to `127.0.0.1`. The `Host` is the one [`HostConfig::public_url`] sets.
pub fn probe_web(port: u16, timeout: Duration) -> WebProbe {
    let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        return WebProbe::NotListening;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request = format!(
        "GET /admin/api/setup HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return WebProbe::NotListening;
    }
    let mut head = [0u8; 32];
    let mut got = 0;
    while got < 12 {
        match stream.read(&mut head[got..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => got += n,
        }
    }
    parse_status(&head[..got]).map_or(WebProbe::NotListening, WebProbe::Answers)
}

fn parse_status(head: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(head).ok()?;
    let rest = text.strip_prefix("HTTP/1.")?;
    let status = rest.get(2..5)?;
    status.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn held() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    #[test]
    fn a_held_port_is_busy_and_a_released_one_is_free() {
        let (listener, port) = held();
        assert!(!is_free(port));
        drop(listener);
        assert!(is_free(port));
        assert!(!is_free(0));
    }

    #[test]
    fn a_wildcard_listener_is_busy_too() {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!is_free(port));
    }

    #[test]
    fn suggestions_skip_busy_and_avoided_ports() {
        let busy = [5001u16, 5002];
        let free = |p: u16| !busy.contains(&p);
        assert_eq!(suggest_with(5000, &[], &free), Some(5003));
        assert_eq!(suggest_with(5000, &[5003], &free), Some(5004));
        assert_eq!(suggest_with(65534, &[], &|_| true), Some(65535));
        assert_eq!(suggest_with(65535, &[], &|_| true), None);
        assert_eq!(suggest_with(5000, &[], &|_| false), None);
    }

    #[test]
    fn reports_which_listener_is_blocked_and_a_free_alternative() {
        let config = HostConfig {
            lan: false,
            game_port: 47624,
            web_port: 8080,
        };
        let free = |p: u16| p != 8080 && p != 8081;
        let conflicts = check_with(&config, &free);
        assert_eq!(
            conflicts,
            vec![Conflict {
                which: PortUse::Web,
                port: 8080,
                same_as_other: false,
                suggestion: Some(8082)
            }]
        );
        let game_busy = check_with(&config, &|p| p != 47624);
        assert_eq!(game_busy[0].which, PortUse::Game);
        assert_eq!(game_busy[0].suggestion, Some(47625));
        assert!(check_with(&config, &|_| true).is_empty());
    }

    #[test]
    fn the_same_port_for_both_listeners_is_a_conflict() {
        let config = HostConfig {
            lan: false,
            game_port: 9000,
            web_port: 9000,
        };
        let conflicts = check_with(&config, &|_| true);
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.iter().all(|c| c.same_as_other));
        assert_ne!(conflicts[0].suggestion, Some(9000));
    }

    fn serve_once(response: &'static str) -> u16 {
        let (listener, port) = held();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                assert!(request.starts_with("GET /admin/api/setup HTTP/1.1\r\n"));
                assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")));
                let _ = stream.write_all(response.as_bytes());
            }
        });
        port
    }

    #[test]
    fn the_web_probe_reads_the_status_line() {
        let port = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}");
        let probe = probe_web(port, Duration::from_secs(2));
        assert_eq!(probe, WebProbe::Answers(200));
        assert!(probe.is_ready());
        let port = serve_once("HTTP/1.1 403 Forbidden\r\n\r\n");
        let probe = probe_web(port, Duration::from_secs(2));
        assert_eq!(probe, WebProbe::Answers(403));
        assert!(!probe.is_ready());
    }

    #[test]
    fn nothing_listening_or_not_http_is_not_listening() {
        let (listener, port) = held();
        drop(listener);
        assert_eq!(
            probe_web(port, Duration::from_millis(300)),
            WebProbe::NotListening
        );
        let port = serve_once("SSH-2.0-banner\r\n");
        assert_eq!(
            probe_web(port, Duration::from_secs(1)),
            WebProbe::NotListening
        );
    }
}
