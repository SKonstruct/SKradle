//! Discord's local IPC, written out by hand (about a page; the only crate used is `serde_json`, already a dependency).
//!
//! The protocol, from Discord's RPC documentation (docs.discord.com/developers/topics/rpc): a Unix socket
//! `discord-ipc-N` (N from 0) in the first of `XDG_RUNTIME_DIR`, `TMPDIR`, `TMP`, `TEMP`, `/tmp`, or on Windows the named
//! pipe `\\?\pipe\discord-ipc-N`. Each frame is a 4-byte opcode and a 4-byte length (the documentation does not name the
//! byte order; every implementation uses little endian, and so does this one) followed by JSON. Opcode 0 is the
//! handshake `{"v":1,"client_id":..}`, answered by an opcode 1 frame carrying the `READY` event; later requests use
//! opcode 1 with `{"cmd":"SET_ACTIVITY","args":{"pid":..,"activity":{..}},"nonce":..}`. Opcode 2 means Discord closed.
//!
//! Untested against a real Discord (none was running); the framing and handshake are tested against a scripted peer.
//! The extra socket folders for Flatpak and Snap Discord are from common practice, not from the documentation.

use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};

use super::{activity, Activity, Presence, PresenceResult, Status};

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
/// A reply larger than this is not from Discord.
const MAX_FRAME: usize = 64 * 1024;
const SOCKET_COUNT: u32 = 10;
#[cfg_attr(not(unix), allow(dead_code))]
const TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) trait Transport: Read + Write + Send {}
impl<T: Read + Write + Send> Transport for T {}

type Connector = Box<dyn FnMut() -> io::Result<Box<dyn Transport>> + Send>;

pub(crate) fn encode_frame(opcode: u32, json: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + json.len());
    out.extend_from_slice(&opcode.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out
}

pub(crate) fn read_frame(reader: &mut impl Read) -> io::Result<(u32, Value)> {
    let mut header = [0u8; 8];
    reader.read_exact(&mut header)?;
    let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized Discord frame",
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    Ok((opcode, value))
}

fn bad(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

pub(crate) fn handshake(io: &mut (impl Read + Write), client_id: &str) -> io::Result<()> {
    let hello = json!({ "v": 1, "client_id": client_id }).to_string();
    io.write_all(&encode_frame(OP_HANDSHAKE, &hello))?;
    let (opcode, reply) = read_frame(io)?;
    match (opcode, reply.get("evt").and_then(Value::as_str)) {
        (OP_FRAME, Some("READY")) => Ok(()),
        (OP_CLOSE, _) => Err(bad(format!(
            "Discord refused the handshake: {}",
            reply
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("closed")
        ))),
        _ => Err(bad("unexpected reply to the Discord handshake")),
    }
}

fn activity_json(activity: &Activity) -> Value {
    let mut value = json!({ "details": activity.details });
    if let Some(state) = &activity.state {
        value["state"] = json!(state);
    }
    if let Some(start) = activity.start {
        value["timestamps"] = json!({ "start": start });
    }
    value
}

/// Sends SET_ACTIVITY (`None` clears the activity) and checks the reply.
pub(crate) fn set_activity(
    io: &mut (impl Read + Write),
    pid: u32,
    activity: Option<&Activity>,
    nonce: &str,
) -> io::Result<()> {
    let request = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": pid, "activity": activity.map(activity_json) },
        "nonce": nonce,
    })
    .to_string();
    io.write_all(&encode_frame(OP_FRAME, &request))?;
    let (opcode, reply) = read_frame(io)?;
    if opcode == OP_CLOSE {
        return Err(bad("Discord closed the connection"));
    }
    if reply.get("evt").and_then(Value::as_str) == Some("ERROR") {
        let message = reply
            .pointer("/data/message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(bad(format!("Discord rejected the activity: {message}")));
    }
    Ok(())
}

/// The socket paths to try, in Discord's order. `env` looks up an environment variable.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn socket_candidates(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(dir) = env(var).filter(|d| !d.is_empty()) {
            let dir = PathBuf::from(dir);
            if var == "XDG_RUNTIME_DIR" {
                dirs.push(dir.clone());
                dirs.push(dir.join("app/com.discordapp.Discord"));
                dirs.push(dir.join("snap.discord"));
            } else {
                dirs.push(dir);
            }
        }
    }
    dirs.push(PathBuf::from("/tmp"));
    let mut out = Vec::new();
    for dir in dirs {
        for n in 0..SOCKET_COUNT {
            let path = dir.join(format!("discord-ipc-{n}"));
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(unix)]
fn connect_local() -> io::Result<Box<dyn Transport>> {
    use std::os::unix::net::UnixStream;
    let env = |name: &str| std::env::var(name).ok();
    for path in socket_candidates(&env) {
        if let Ok(stream) = UnixStream::connect(&path) {
            stream.set_read_timeout(Some(TIMEOUT))?;
            stream.set_write_timeout(Some(TIMEOUT))?;
            return Ok(Box::new(stream));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord is not running",
    ))
}

#[cfg(windows)]
fn connect_local() -> io::Result<Box<dyn Transport>> {
    for n in 0..SOCKET_COUNT {
        let path = format!(r"\\?\pipe\discord-ipc-{n}");
        if let Ok(pipe) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
        {
            return Ok(Box::new(pipe));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Discord is not running",
    ))
}

#[cfg(not(any(unix, windows)))]
fn connect_local() -> io::Result<Box<dyn Transport>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no local IPC on this platform",
    ))
}

/// Presence over Discord's local IPC. Connects on first use and again after any failure.
pub struct DiscordPresence {
    client_id: String,
    connect: Connector,
    link: Option<Box<dyn Transport>>,
    nonce: u64,
}

impl DiscordPresence {
    /// `client_id` must be the owner's own Discord application id (see [`super::from_client_id`]).
    pub fn new(client_id: &str) -> Self {
        Self::with_connector(client_id, Box::new(connect_local))
    }

    pub(crate) fn with_connector(client_id: &str, connect: Connector) -> Self {
        Self {
            client_id: client_id.to_owned(),
            connect,
            link: None,
            nonce: 0,
        }
    }

    fn send(&mut self, activity: Option<&Activity>) -> PresenceResult {
        if self.link.is_none() {
            let mut link = match (self.connect)() {
                Ok(link) => link,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return PresenceResult::Disabled,
                Err(e) => return PresenceResult::Failed(format!("connect: {e}")),
            };
            if let Err(e) = handshake(&mut link, &self.client_id) {
                return PresenceResult::Failed(format!("handshake: {e}"));
            }
            self.link = Some(link);
        }
        self.nonce += 1;
        let nonce = format!("skradle-{}", self.nonce);
        let Some(link) = self.link.as_mut() else {
            return PresenceResult::Disabled;
        };
        match set_activity(link, std::process::id(), activity, &nonce) {
            Ok(()) => PresenceResult::Updated,
            Err(e) => {
                self.link = None;
                PresenceResult::Failed(e.to_string())
            }
        }
    }
}

impl Presence for DiscordPresence {
    fn update(&mut self, status: &Status) -> PresenceResult {
        self.send(Some(&activity(status)))
    }

    fn clear(&mut self) -> PresenceResult {
        self.send(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presence::State;
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::sync::{Arc, Mutex};

    /// A peer that answers with scripted frames and records what it was sent.
    struct Script {
        replies: Cursor<Vec<u8>>,
        sent: Arc<Mutex<Vec<u8>>>,
    }
    impl Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.replies.read(buf)
        }
    }
    impl Write for Script {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.sent.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn ready() -> Vec<u8> {
        encode_frame(OP_FRAME, r#"{"cmd":"DISPATCH","evt":"READY","data":{}}"#)
    }
    fn ok() -> Vec<u8> {
        encode_frame(OP_FRAME, r#"{"cmd":"SET_ACTIVITY","data":{},"evt":null}"#)
    }

    fn frames(mut bytes: &[u8]) -> Vec<(u32, Value)> {
        let mut out = Vec::new();
        while !bytes.is_empty() {
            out.push(read_frame(&mut bytes).unwrap());
        }
        out
    }

    #[test]
    fn frames_are_little_endian_opcode_length_json() {
        let bytes = encode_frame(1, "{}");
        assert_eq!(bytes, [1, 0, 0, 0, 2, 0, 0, 0, b'{', b'}']);
        let (op, value) = read_frame(&mut bytes.as_slice()).unwrap();
        assert_eq!((op, value), (1, json!({})));
    }

    #[test]
    fn oversized_and_truncated_frames_are_errors() {
        let mut huge = vec![1, 0, 0, 0];
        huge.extend_from_slice(&(MAX_FRAME as u32 + 1).to_le_bytes());
        assert!(read_frame(&mut huge.as_slice()).is_err());
        assert!(read_frame(&mut [1u8, 0, 0].as_slice()).is_err());
        let mut short = encode_frame(1, "{\"a\":1}");
        short.truncate(10);
        assert!(read_frame(&mut short.as_slice()).is_err());
    }

    #[test]
    fn handshake_sends_the_client_id_and_wants_ready() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut peer = Script {
            replies: Cursor::new(ready()),
            sent: sent.clone(),
        };
        handshake(&mut peer, "123456789012345678").unwrap();
        let sent = frames(&sent.lock().unwrap());
        assert_eq!(
            sent,
            [(0, json!({"v": 1, "client_id": "123456789012345678"}))]
        );
    }

    #[test]
    fn handshake_failures() {
        let closed = encode_frame(OP_CLOSE, r#"{"code":4000,"message":"Invalid Client ID"}"#);
        let mut peer = Script {
            replies: Cursor::new(closed),
            sent: Default::default(),
        };
        let err = handshake(&mut peer, "1").unwrap_err();
        assert!(err.to_string().contains("Invalid Client ID"));

        let mut peer = Script {
            replies: Cursor::new(encode_frame(OP_FRAME, r#"{"evt":"OTHER"}"#)),
            sent: Default::default(),
        };
        assert!(handshake(&mut peer, "1").is_err());
        let mut silent = Script {
            replies: Cursor::new(Vec::new()),
            sent: Default::default(),
        };
        assert!(handshake(&mut silent, "1").is_err());
    }

    #[test]
    fn set_activity_request_shape() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut peer = Script {
            replies: Cursor::new(ok()),
            sent: sent.clone(),
        };
        let a = Activity {
            details: "Playing Spiral Knights".into(),
            state: Some("on Emberlight".into()),
            start: Some(1700000000),
        };
        set_activity(&mut peer, 42, Some(&a), "n1").unwrap();
        let sent = frames(&sent.lock().unwrap());
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, OP_FRAME);
        assert_eq!(
            sent[0].1,
            json!({
                "cmd": "SET_ACTIVITY",
                "nonce": "n1",
                "args": {"pid": 42, "activity": {
                    "details": "Playing Spiral Knights",
                    "state": "on Emberlight",
                    "timestamps": {"start": 1700000000u64}
                }}
            })
        );
    }

    #[test]
    fn clearing_sends_a_null_activity_and_optional_fields_are_omitted() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let mut peer = Script {
            replies: Cursor::new([ok(), ok()].concat()),
            sent: sent.clone(),
        };
        set_activity(&mut peer, 1, None, "a").unwrap();
        let a = Activity {
            details: "In the launcher".into(),
            state: None,
            start: None,
        };
        set_activity(&mut peer, 1, Some(&a), "b").unwrap();
        let sent = frames(&sent.lock().unwrap());
        assert_eq!(sent[0].1["args"]["activity"], Value::Null);
        assert_eq!(
            sent[1].1["args"]["activity"],
            json!({"details": "In the launcher"})
        );
    }

    #[test]
    fn discord_errors_are_reported() {
        let mut peer = Script {
            replies: Cursor::new(encode_frame(
                OP_FRAME,
                r#"{"evt":"ERROR","data":{"code":4000,"message":"bad activity"}}"#,
            )),
            sent: Default::default(),
        };
        let err = set_activity(&mut peer, 1, None, "x").unwrap_err();
        assert!(err.to_string().contains("bad activity"));
    }

    #[test]
    fn socket_candidates_follow_discords_order() {
        let env = |name: &str| match name {
            "XDG_RUNTIME_DIR" => Some("/run/user/1000".to_owned()),
            "TMPDIR" => Some("/var/tmp".to_owned()),
            _ => None,
        };
        let paths = socket_candidates(&env);
        assert_eq!(paths[0], PathBuf::from("/run/user/1000/discord-ipc-0"));
        assert_eq!(paths[9], PathBuf::from("/run/user/1000/discord-ipc-9"));
        assert_eq!(
            paths[10],
            PathBuf::from("/run/user/1000/app/com.discordapp.Discord/discord-ipc-0")
        );
        let tmpdir = paths
            .iter()
            .position(|p| p == &PathBuf::from("/var/tmp/discord-ipc-0"))
            .unwrap();
        let tmp = paths
            .iter()
            .position(|p| p == &PathBuf::from("/tmp/discord-ipc-0"))
            .unwrap();
        assert!(tmpdir < tmp);
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(paths.len(), unique.len());
    }

    fn connector(script: Vec<Vec<u8>>, sent: Arc<Mutex<Vec<u8>>>) -> Connector {
        let mut script: VecDeque<Vec<u8>> = script.into();
        Box::new(move || match script.pop_front() {
            Some(replies) => Ok(Box::new(Script {
                replies: Cursor::new(replies),
                sent: sent.clone(),
            }) as Box<dyn Transport>),
            None => Err(io::Error::new(io::ErrorKind::NotFound, "gone")),
        })
    }

    #[test]
    fn presence_connects_once_and_reconnects_after_a_failure() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        // First link: ready + one ok, then silence (the next request fails). Second link works.
        let first = [ready(), ok()].concat();
        let second = [ready(), ok()].concat();
        let mut presence = DiscordPresence::with_connector(
            "123456789012345678",
            connector(vec![first, second], sent.clone()),
        );
        let playing = Status::new(State::Playing);
        assert_eq!(presence.update(&playing), PresenceResult::Updated);
        assert!(matches!(
            presence.update(&playing),
            PresenceResult::Failed(_)
        ));
        assert_eq!(presence.update(&playing), PresenceResult::Updated);
        // Out of links: Discord "went away", which is a quiet no-op.
        presence.link = None;
        assert_eq!(presence.clear(), PresenceResult::Disabled);
        let opcodes: Vec<u32> = frames(&sent.lock().unwrap()).iter().map(|f| f.0).collect();
        assert_eq!(opcodes, [0, 1, 1, 0, 1]);
    }

    #[test]
    fn a_failed_handshake_is_reported_not_raised() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let closed = encode_frame(OP_CLOSE, r#"{"message":"nope"}"#);
        let mut presence = DiscordPresence::with_connector("1", connector(vec![closed], sent));
        match presence.update(&Status::new(State::Idle)) {
            PresenceResult::Failed(message) => assert!(message.contains("handshake")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn works_over_a_real_unix_socket() {
        use std::os::unix::net::UnixStream;
        let (client, mut server) = UnixStream::pair().unwrap();
        let peer = std::thread::spawn(move || {
            let (op, hello) = read_frame(&mut server).unwrap();
            assert_eq!((op, hello["client_id"].as_str()), (0, Some("77")));
            server.write_all(&ready()).unwrap();
            let (op, request) = read_frame(&mut server).unwrap();
            assert_eq!((op, request["cmd"].as_str()), (1, Some("SET_ACTIVITY")));
            server.write_all(&ok()).unwrap();
        });
        let mut slot = Some(client);
        let mut presence = DiscordPresence::with_connector(
            "77",
            Box::new(move || {
                slot.take()
                    .map(|s| Box::new(s) as Box<dyn Transport>)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "once"))
            }),
        );
        assert_eq!(
            presence.update(&Status::new(State::Launching)),
            PresenceResult::Updated
        );
        peer.join().unwrap();
    }
}
