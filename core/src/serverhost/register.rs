//! Turning a running (or at least once started) server into something the launcher can connect to: the
//! `servers::Server` entry for the "Local Server", and the join details to hand to other players.
//!
//! The engine writes its public key to `data/keys/public.txt` the first time it starts (`modulus#exponent` in hex,
//! the same shape as the official key). Only that file is ever read from `data/keys/`; the private key stays
//! unread. The local entry points at `127.0.0.1` and the configured game port and sets the content URL to the
//! engine's own News listener, so the game finds the server's News there.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use crate::servers::{self, Problem, Server, ServerList};

use super::config::{HostConfig, LOOPBACK};
use super::layout::ServerLayout;

/// The stable id of the local server entry, so saving it again replaces it.
pub const LOCAL_SERVER_ID: &str = "local-server";
pub const LOCAL_SERVER_NAME: &str = "Local Server";

#[derive(Debug)]
pub enum RegisterError {
    /// The engine has not generated its key yet (it has never run to the point of loading its keys).
    NoKey,
    Unreadable(std::io::Error),
    /// The key file is not `modulus#exponent` in hex.
    BadKey,
    /// `servers::validate` refused the entry.
    Invalid(Problem),
}

impl fmt::Display for RegisterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegisterError::NoKey => write!(f, "the server has not generated its public key yet"),
            RegisterError::Unreadable(e) => write!(f, "cannot read the public key: {e}"),
            RegisterError::BadKey => write!(f, "the public key file is not a valid key"),
            RegisterError::Invalid(p) => write!(f, "the server entry is invalid ({p:?})"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// The server's public key text, trimmed.
pub fn read_public_key(layout: &ServerLayout) -> Result<String, RegisterError> {
    let text = match std::fs::read_to_string(layout.public_key()) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(RegisterError::NoKey),
        Err(e) => return Err(RegisterError::Unreadable(e)),
    };
    let key = text.trim().to_owned();
    // Reuse the server list's own check of the key shape.
    let probe = Server {
        id: "probe".into(),
        name: "probe".into(),
        host: LOOPBACK.into(),
        port: 1,
        public_key: key.clone(),
        client_url: String::new(),
        content_url: String::new(),
    };
    match servers::validate(&probe, &[]) {
        Some(Problem::Key) => Err(RegisterError::BadKey),
        _ if key.is_empty() => Err(RegisterError::BadKey),
        _ => Ok(key),
    }
}

/// The entry for the local server: loopback address, the configured game port, the generated key and the News
/// listener as content URL.
pub fn local_server(layout: &ServerLayout, config: &HostConfig) -> Result<Server, RegisterError> {
    let server = Server {
        id: LOCAL_SERVER_ID.to_owned(),
        name: LOCAL_SERVER_NAME.to_owned(),
        host: LOOPBACK.to_owned(),
        port: config.game_port,
        public_key: read_public_key(layout)?,
        client_url: String::new(),
        content_url: config.content_url(LOOPBACK),
    };
    match servers::validate(&server, &[]) {
        None => Ok(server),
        Some(p) => Err(RegisterError::Invalid(p)),
    }
}

/// The same entry before the engine has made its key: listed so the player can choose "Local Server" before the
/// first start (the first start fills the key in).
pub fn local_server_keyless(config: &HostConfig) -> Result<Server, RegisterError> {
    let server = Server {
        id: LOCAL_SERVER_ID.to_owned(),
        name: LOCAL_SERVER_NAME.to_owned(),
        host: LOOPBACK.to_owned(),
        port: config.game_port,
        public_key: String::new(),
        client_url: String::new(),
        content_url: config.content_url(LOOPBACK),
    };
    match servers::validate(&server, &[]) {
        None => Ok(server),
        Some(p) => Err(RegisterError::Invalid(p)),
    }
}

/// Adds the local server to the list or replaces the one already there (same id), keeping the selection. A
/// different server the player named "Local Server" is not overwritten: the call fails with `Duplicate`.
pub fn register(list: &mut ServerList, server: Server) -> Result<(), RegisterError> {
    list.save(server).map_err(RegisterError::Invalid)
}

/// What another player needs to connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinInfo {
    pub host: String,
    pub port: u16,
    pub public_key: String,
    pub content_url: String,
    /// False when the server only listens on this computer, so nobody else can connect yet.
    pub reachable_by_others: bool,
}

/// The join details for `host` (a LAN or VPN address or a name the player chose). `None` for `host` uses the
/// computer's primary address, if it can be found.
pub fn join_info(
    layout: &ServerLayout,
    config: &HostConfig,
    host: Option<&str>,
) -> Result<JoinInfo, RegisterError> {
    let host = match host.map(str::trim).filter(|h| !h.is_empty()) {
        Some(host) => host.to_owned(),
        None => primary_address().map_or_else(|| LOOPBACK.to_owned(), |ip| ip.to_string()),
    };
    let probe = Server {
        id: "probe".into(),
        name: "probe".into(),
        host: host.clone(),
        port: config.game_port,
        public_key: String::new(),
        client_url: String::new(),
        content_url: String::new(),
    };
    if let Some(problem) = servers::validate(&probe, &[]) {
        return Err(RegisterError::Invalid(problem));
    }
    Ok(JoinInfo {
        content_url: config.content_url(&host),
        host,
        port: config.game_port,
        public_key: read_public_key(layout)?,
        reachable_by_others: config.lan,
    })
}

/// The address this computer would use to reach the internet: a UDP socket "connected" to a documentation address
/// (RFC 5737; nothing is sent) reports which local address the system would pick. This is the LAN address on a
/// typical home network; behind a VPN, or with several networks, the player should choose the address themselves.
pub fn primary_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))).ok()?;
    socket.connect(SocketAddr::from(([192, 0, 2, 1], 9))).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "a5ed0dc3892b9472#10001";

    fn with_key(key: Option<&str>) -> (tempfile::TempDir, ServerLayout) {
        let dir = tempfile::tempdir().unwrap();
        let layout = ServerLayout::new(dir.path());
        std::fs::create_dir_all(layout.keys_dir()).unwrap();
        std::fs::write(
            layout.keys_dir().join("private.pk8"),
            b"PRIVATE-DO-NOT-READ",
        )
        .unwrap();
        if let Some(key) = key {
            std::fs::write(layout.public_key(), key).unwrap();
        }
        (dir, layout)
    }

    #[test]
    fn the_local_entry_points_at_loopback_with_the_generated_key() {
        let (_d, layout) = with_key(Some(&format!("  {KEY}\n")));
        let config = HostConfig {
            lan: true,
            game_port: 50000,
            web_port: 50001,
        };
        let server = local_server(&layout, &config).unwrap();
        assert_eq!(server.id, LOCAL_SERVER_ID);
        assert_eq!(server.name, "Local Server");
        assert_eq!(server.host, "127.0.0.1");
        assert_eq!(server.port, 50000);
        assert_eq!(server.public_key, KEY);
        assert_eq!(server.content_url, "http://127.0.0.1:50001/");
        assert_eq!(servers::validate(&server, &[]), None);
        let props = servers::desired_properties(Some(&server));
        assert_eq!(props["content_url"], "http://127.0.0.1:50001/");
        assert_eq!(props["server_ports"], "50000");
        assert_eq!(props["key.public"], KEY);
    }

    #[test]
    fn a_missing_or_damaged_key_is_reported() {
        let (_d, layout) = with_key(None);
        assert!(matches!(
            local_server(&layout, &HostConfig::default()),
            Err(RegisterError::NoKey)
        ));
        for bad in ["", "not a key", "abc#", "#10001", "zz#10001"] {
            std::fs::write(layout.public_key(), bad).unwrap();
            assert!(
                matches!(read_public_key(&layout), Err(RegisterError::BadKey)),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn registering_twice_replaces_and_keeps_the_selection_and_other_servers() {
        let (_d, layout) = with_key(Some(KEY));
        let mut list = ServerList::default();
        register(
            &mut list,
            Server {
                id: "other".into(),
                name: "Friend".into(),
                host: "play.example.org".into(),
                port: 47624,
                public_key: String::new(),
                client_url: String::new(),
                content_url: String::new(),
            },
        )
        .unwrap();
        let mut config = HostConfig::default();
        register(&mut list, local_server(&layout, &config).unwrap()).unwrap();
        list.select(Some(LOCAL_SERVER_ID));
        config.game_port = 40000;
        register(&mut list, local_server(&layout, &config).unwrap()).unwrap();
        assert_eq!(list.servers.len(), 2);
        assert_eq!(list.selected_server().unwrap().port, 40000);
    }

    #[test]
    fn a_foreign_server_with_the_same_name_is_not_overwritten() {
        let (_d, layout) = with_key(Some(KEY));
        let mut list = ServerList::default();
        list.save(Server {
            id: "mine".into(),
            name: LOCAL_SERVER_NAME.into(),
            host: "play.example.org".into(),
            port: 1,
            public_key: String::new(),
            client_url: String::new(),
            content_url: String::new(),
        })
        .unwrap();
        let err = register(
            &mut list,
            local_server(&layout, &HostConfig::default()).unwrap(),
        )
        .unwrap_err();
        assert!(matches!(err, RegisterError::Invalid(Problem::Duplicate)));
        assert_eq!(list.servers.len(), 1);
    }

    #[test]
    fn join_info_carries_the_chosen_host_the_port_and_the_public_key_only() {
        let (_d, layout) = with_key(Some(KEY));
        let config = HostConfig {
            lan: true,
            game_port: 47624,
            web_port: 8080,
        };
        let info = join_info(&layout, &config, Some(" 10.8.0.2 ")).unwrap();
        assert_eq!(
            info,
            JoinInfo {
                host: "10.8.0.2".into(),
                port: 47624,
                public_key: KEY.into(),
                content_url: "http://10.8.0.2:8080/".into(),
                reachable_by_others: true,
            }
        );
        assert!(!format!("{info:?}").contains("PRIVATE"));
        let local_only = join_info(&layout, &HostConfig::default(), Some("10.8.0.2")).unwrap();
        assert!(!local_only.reachable_by_others);
        assert!(matches!(
            join_info(&layout, &config, Some("bad host/x")),
            Err(RegisterError::Invalid(Problem::Host))
        ));
    }

    #[test]
    fn the_primary_address_is_never_loopback() {
        if let Some(ip) = primary_address() {
            assert!(!ip.is_loopback() && !ip.is_unspecified());
        }
        let (_d, layout) = with_key(Some(KEY));
        let info = join_info(&layout, &HostConfig::default(), None).unwrap();
        assert!(!info.host.is_empty());
    }
}
