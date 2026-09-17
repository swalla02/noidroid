//! How a recorded program reaches the engine.
//!
//! A Unix domain socket where there is one, loopback TCP where there is not (#32).
//! The protocol on top — newline-delimited JSON, one request and one response at a
//! time — is identical, so nothing above this module knows which it got.
//!
//! TCP brings one thing a socket file does not have: anyone on the machine can connect
//! to a loopback port. So a TCP listener mints a token, hands it to the child in its
//! environment, and accepts a connection only once its first line is a `hello` carrying
//! that token. Anything else is dropped and the listener keeps waiting for the child.
//! The token is as private as the child's environment, which on every supported
//! platform means other users cannot read it.
//!
//! `NOIDROID_TRANSPORT=tcp` selects TCP on a Unix machine too. That is how the TCP path
//! is tested on the platforms CI can run everything on, rather than only on Windows.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a TCP connection gets to present its token before it is dropped. A real
/// client sends `hello` immediately after connecting.
const HANDSHAKE: Duration = Duration::from_secs(5);

pub enum Listener {
    #[cfg(unix)]
    Unix {
        listener: UnixListener,
        path: PathBuf,
    },
    Tcp {
        listener: TcpListener,
        token: String,
    },
}

pub enum Conn {
    #[cfg(unix)]
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Listener {
    /// Bind the transport this platform should use. Non-blocking, so the caller can
    /// notice a child that exits without ever connecting.
    pub fn bind() -> io::Result<Listener> {
        let tcp = cfg!(not(unix)) || std::env::var("NOIDROID_TRANSPORT").as_deref() == Ok("tcp");
        if tcp {
            return Listener::bind_tcp();
        }
        #[cfg(unix)]
        {
            let path = unique_socket_path();
            let _ = std::fs::remove_file(&path);
            let listener = UnixListener::bind(&path)?;
            listener.set_nonblocking(true)?;
            Ok(Listener::Unix { listener, path })
        }
        #[cfg(not(unix))]
        unreachable!("non-Unix platforms always take the TCP path")
    }

    pub fn bind_tcp() -> io::Result<Listener> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        Ok(Listener::Tcp {
            listener,
            token: mint_token(),
        })
    }

    /// What the child needs in its environment to find this listener.
    pub fn child_env(&self) -> io::Result<Vec<(&'static str, String)>> {
        Ok(match self {
            #[cfg(unix)]
            Listener::Unix { path, .. } => {
                vec![("NOIDROID_SOCKET", path.display().to_string())]
            }
            Listener::Tcp { listener, token } => vec![
                ("NOIDROID_ADDRESS", listener.local_addr()?.to_string()),
                ("NOIDROID_TOKEN", token.clone()),
            ],
        })
    }

    /// A connection, if one is waiting. `Ok(None)` means nobody is there yet — or, for
    /// TCP, that whoever was there did not present the token and has been dropped.
    ///
    /// The second value is a line already read off the connection: the handshake, for
    /// TCP. The caller must handle it as the first request.
    pub fn accept(&self) -> io::Result<Option<(Conn, Option<String>)>> {
        match self {
            #[cfg(unix)]
            Listener::Unix { listener, .. } => match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false)?;
                    Ok(Some((Conn::Unix(stream), None)))
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(e) => Err(e),
            },
            Listener::Tcp { listener, token } => match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false)?;
                    Ok(handshake(stream, token).map(|(s, line)| (Conn::Tcp(s), Some(line))))
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
                Err(e) => Err(e),
            },
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Listener::Unix { path, .. } = self {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Read the first line byte by byte — a buffered reader could swallow the start of the
/// next request — and keep the connection only if it is a `hello` with the token.
fn handshake(mut stream: TcpStream, token: &str) -> Option<(TcpStream, String)> {
    stream.set_read_timeout(Some(HANDSHAKE)).ok()?;
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while line.len() < 64 * 1024 {
        match stream.read(&mut byte) {
            Ok(1) if byte[0] == b'\n' => break,
            Ok(1) => line.push(byte[0]),
            _ => return None,
        }
    }
    let text = String::from_utf8(line).ok()?;
    let hello: serde_json::Value = serde_json::from_str(&text).ok()?;
    let presented = hello.get("token").and_then(|t| t.as_str())?;
    if hello.get("op").and_then(|o| o.as_str()) != Some("hello") || presented != token {
        return None;
    }
    stream.set_read_timeout(None).ok()?;
    Some((stream, text))
}

impl Conn {
    pub fn try_clone(&self) -> io::Result<Conn> {
        Ok(match self {
            #[cfg(unix)]
            Conn::Unix(s) => Conn::Unix(s.try_clone()?),
            Conn::Tcp(s) => Conn::Tcp(s.try_clone()?),
        })
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Conn::Unix(s) => s.read(buf),
            Conn::Tcp(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(unix)]
            Conn::Unix(s) => s.write(buf),
            Conn::Tcp(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(unix)]
            Conn::Unix(s) => s.flush(),
            Conn::Tcp(s) => s.flush(),
        }
    }
}

fn mixed(tag: &str) -> blake3::Hash {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    blake3::hash(
        format!(
            "{tag}-{}-{nanos}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        )
        .as_bytes(),
    )
}

/// Unguessable to a different local user poking at a loopback port. The pid and the
/// clock alone would not be: both can be narrowed down from outside. So it also mixes in
/// operating-system randomness, taken from the keys std seeds every `RandomState` with,
/// which needs no dependency on any platform.
fn mint_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut seed = String::new();
    for _ in 0..4 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(0);
        seed.push_str(&format!("{:016x}", hasher.finish()));
    }
    blake3::hash(format!("{seed}-{}", mixed("token").to_hex()).as_bytes())
        .to_hex()
        .to_string()
}

#[cfg(unix)]
fn unique_socket_path() -> PathBuf {
    // Kept short and in the system temp dir: `sun_path` is limited to ~104 bytes and
    // a repository can live at an arbitrarily deep path.
    let short = &mixed("socket").to_hex()[..16];
    std::env::temp_dir().join(format!("nd-{short}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    fn wait_for(listener: &Listener) -> Option<(Conn, Option<String>)> {
        for _ in 0..200 {
            if let Some(found) = listener.accept().unwrap() {
                return Some(found);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        None
    }

    fn address(listener: &Listener) -> (String, String) {
        let env = listener.child_env().unwrap();
        let get = |k| env.iter().find(|(key, _)| *key == k).unwrap().1.clone();
        (get("NOIDROID_ADDRESS"), get("NOIDROID_TOKEN"))
    }

    #[test]
    fn a_tcp_connection_with_the_token_is_accepted_and_its_hello_handed_over() {
        let listener = Listener::bind_tcp().unwrap();
        let (addr, token) = address(&listener);
        let mut client = TcpStream::connect(&addr).unwrap();
        writeln!(client, r#"{{"op":"hello","client":"t","token":"{token}"}}"#).unwrap();

        let (mut conn, first) = wait_for(&listener).expect("the real client gets in");
        assert!(first.unwrap().contains("\"hello\""));

        // And the connection still carries the protocol in both directions.
        conn.write_all(b"{\"ok\":true}\n").unwrap();
        let mut reply = String::new();
        BufReader::new(client.try_clone().unwrap())
            .read_line(&mut reply)
            .unwrap();
        assert_eq!(reply.trim(), "{\"ok\":true}");
    }

    #[test]
    fn a_tcp_connection_without_the_token_is_dropped_and_the_real_one_still_gets_in() {
        let listener = Listener::bind_tcp().unwrap();
        let (addr, token) = address(&listener);

        let mut stranger = TcpStream::connect(&addr).unwrap();
        writeln!(stranger, r#"{{"op":"hello","client":"x","token":"guess"}}"#).unwrap();
        let mut silent = TcpStream::connect(&addr).unwrap();
        writeln!(silent, r#"{{"op":"call","target":"x"}}"#).unwrap();

        let mut client = TcpStream::connect(&addr).unwrap();
        writeln!(client, r#"{{"op":"hello","client":"t","token":"{token}"}}"#).unwrap();

        let (_, first) = wait_for(&listener).expect("the real client gets in");
        assert!(
            first.unwrap().contains(&token),
            "the accepted connection is the one that presented the token"
        );
    }
}
