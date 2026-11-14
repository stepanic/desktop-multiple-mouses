//! Wire protocol between the Windows capture app and the macOS agent.
//!
//! Connection flow (TCP, client = Windows, server = Mac):
//!   client -> server: MAGIC (4 bytes) | VERSION (u8) | token_len (u8) | token bytes
//!   server -> client: one status byte (HELLO_OK or HELLO_BAD_TOKEN / HELLO_BAD_VERSION)
//!   then client streams `Msg` frames. Every frame is: tag (u8) + fixed-size payload.

use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 4] = b"MMUS";
pub const VERSION: u8 = 1;
pub const DEFAULT_PORT: u16 = 47800;

pub const HELLO_OK: u8 = 0;
pub const HELLO_BAD_TOKEN: u8 = 1;
pub const HELLO_BAD_VERSION: u8 = 2;

/// Client sends a ping at least this often; the server drops the connection
/// (and releases all keys/buttons) if nothing arrives for `SERVER_TIMEOUT_SECS`.
pub const PING_INTERVAL_SECS: u64 = 2;
pub const SERVER_TIMEOUT_SECS: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left = 0,
    Right = 1,
    Middle = 2,
    Back = 3,
    Forward = 4,
}

impl Button {
    pub fn from_u8(v: u8) -> Option<Button> {
        Some(match v {
            0 => Button::Left,
            1 => Button::Right,
            2 => Button::Middle,
            3 => Button::Back,
            4 => Button::Forward,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    /// Relative mouse movement in raw device counts.
    Move { dx: i32, dy: i32 },
    Button { button: Button, down: bool },
    /// Wheel deltas in Windows units (120 = one notch). Positive vertical = away
    /// from the user (scroll up), positive horizontal = right.
    Wheel { vertical: i32, horizontal: i32 },
    /// PC scan code (set 1). `e0` marks extended keys (arrows, right ctrl, ...).
    Key { scancode: u16, e0: bool, down: bool },
    /// Release every key and button currently held on the remote side.
    ReleaseAll,
    Ping,
}

const TAG_MOVE: u8 = 1;
const TAG_BUTTON: u8 = 2;
const TAG_WHEEL: u8 = 3;
const TAG_KEY: u8 = 4;
const TAG_RELEASE_ALL: u8 = 5;
const TAG_PING: u8 = 6;

impl Msg {
    pub fn encode(&self, out: &mut Vec<u8>) {
        match *self {
            Msg::Move { dx, dy } => {
                out.push(TAG_MOVE);
                out.extend_from_slice(&dx.to_le_bytes());
                out.extend_from_slice(&dy.to_le_bytes());
            }
            Msg::Button { button, down } => {
                out.push(TAG_BUTTON);
                out.push(button as u8);
                out.push(down as u8);
            }
            Msg::Wheel { vertical, horizontal } => {
                out.push(TAG_WHEEL);
                out.extend_from_slice(&vertical.to_le_bytes());
                out.extend_from_slice(&horizontal.to_le_bytes());
            }
            Msg::Key { scancode, e0, down } => {
                out.push(TAG_KEY);
                out.extend_from_slice(&scancode.to_le_bytes());
                out.push((e0 as u8) | ((down as u8) << 1));
            }
            Msg::ReleaseAll => out.push(TAG_RELEASE_ALL),
            Msg::Ping => out.push(TAG_PING),
        }
    }

    pub fn read_from<R: Read>(r: &mut R) -> io::Result<Msg> {
        let mut tag = [0u8; 1];
        r.read_exact(&mut tag)?;
        match tag[0] {
            TAG_MOVE => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                Ok(Msg::Move {
                    dx: i32::from_le_bytes(b[0..4].try_into().unwrap()),
                    dy: i32::from_le_bytes(b[4..8].try_into().unwrap()),
                })
            }
            TAG_BUTTON => {
                let mut b = [0u8; 2];
                r.read_exact(&mut b)?;
                let button = Button::from_u8(b[0]).ok_or_else(|| bad("bad button"))?;
                Ok(Msg::Button { button, down: b[1] != 0 })
            }
            TAG_WHEEL => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                Ok(Msg::Wheel {
                    vertical: i32::from_le_bytes(b[0..4].try_into().unwrap()),
                    horizontal: i32::from_le_bytes(b[4..8].try_into().unwrap()),
                })
            }
            TAG_KEY => {
                let mut b = [0u8; 3];
                r.read_exact(&mut b)?;
                Ok(Msg::Key {
                    scancode: u16::from_le_bytes([b[0], b[1]]),
                    e0: b[2] & 1 != 0,
                    down: b[2] & 2 != 0,
                })
            }
            TAG_RELEASE_ALL => Ok(Msg::ReleaseAll),
            TAG_PING => Ok(Msg::Ping),
            t => Err(bad(&format!("unknown message tag {t}"))),
        }
    }
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

pub fn write_hello<W: Write>(w: &mut W, token: &str) -> io::Result<()> {
    let token = token.as_bytes();
    if token.len() > 255 {
        return Err(bad("token too long"));
    }
    let mut buf = Vec::with_capacity(6 + token.len());
    buf.extend_from_slice(MAGIC);
    buf.push(VERSION);
    buf.push(token.len() as u8);
    buf.extend_from_slice(token);
    w.write_all(&buf)
}

/// Reads the client hello. Returns (version, token).
pub fn read_hello<R: Read>(r: &mut R) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 6];
    r.read_exact(&mut head)?;
    if &head[0..4] != MAGIC {
        return Err(bad("bad magic"));
    }
    let mut token = vec![0u8; head[5] as usize];
    r.read_exact(&mut token)?;
    Ok((head[4], token))
}

/// Comparison whose duration does not depend on where the inputs differ.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msgs = [
            Msg::Move { dx: -5, dy: 1200 },
            Msg::Button { button: Button::Forward, down: true },
            Msg::Wheel { vertical: -120, horizontal: 240 },
            Msg::Key { scancode: 0x4B, e0: true, down: false },
            Msg::ReleaseAll,
            Msg::Ping,
        ];
        let mut buf = Vec::new();
        for m in &msgs {
            m.encode(&mut buf);
        }
        let mut cur = io::Cursor::new(buf);
        for m in &msgs {
            assert_eq!(Msg::read_from(&mut cur).unwrap(), *m);
        }
    }

    #[test]
    fn hello_roundtrip() {
        let mut buf = Vec::new();
        write_hello(&mut buf, "secret").unwrap();
        let (v, t) = read_hello(&mut io::Cursor::new(buf)).unwrap();
        assert_eq!(v, VERSION);
        assert_eq!(t, b"secret");
    }
}
