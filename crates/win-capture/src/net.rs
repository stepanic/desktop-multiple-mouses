//! Background thread that keeps a TCP connection to the Mac agent and streams events.

use mm_proto::Msg;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub fn run(address: String, token: String, rx: Receiver<Msg>) {
    let mut last_error = String::new();
    loop {
        match connect(&address, &token) {
            Ok(stream) => {
                last_error.clear();
                println!("[mreža] Spojeno na Mac ({address}).");
                let reason = stream_events(stream, &rx);
                match reason {
                    Some(e) => println!("[mreža] Veza prekinuta: {e}. Pokušavam ponovno..."),
                    None => return, // main thread is gone
                }
            }
            Err(e) => {
                if e != last_error {
                    println!("[mreža] Ne mogu se spojiti na {address}: {e}. Pokušavam svake {} s...", RETRY_DELAY.as_secs());
                    last_error = e;
                }
            }
        }
        // While disconnected, discard events so nothing stale replays after reconnect.
        let until = Instant::now() + RETRY_DELAY;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

fn connect(address: &str, token: &str) -> Result<TcpStream, String> {
    let addrs: Vec<_> = address
        .to_socket_addrs()
        .map_err(|e| format!("ne mogu razriješiti adresu ({e})"))?
        .collect();
    let mut last = String::from("nema adrese");
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, Duration::from_secs(3)) {
            Ok(mut s) => {
                let _ = s.set_nodelay(true);
                s.set_read_timeout(Some(Duration::from_secs(5))).ok();
                mm_proto::write_hello(&mut s, token).map_err(|e| e.to_string())?;
                let mut status = [0u8; 1];
                s.read_exact(&mut status).map_err(|e| format!("nema odgovora od agenta ({e})"))?;
                return match status[0] {
                    mm_proto::HELLO_OK => Ok(s),
                    mm_proto::HELLO_BAD_TOKEN => Err("Mac agent je odbio token (provjeri token u multi-mouse.toml)".into()),
                    mm_proto::HELLO_BAD_VERSION => Err("različite verzije Windows aplikacije i Mac agenta".into()),
                    x => Err(format!("nepoznat odgovor agenta {x}")),
                };
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// Streams events until a write fails (returns the error) or the sender is dropped (None).
fn stream_events(mut s: TcpStream, rx: &Receiver<Msg>) -> Option<String> {
    let ping = Duration::from_secs(mm_proto::PING_INTERVAL_SECS);
    let mut buf = Vec::with_capacity(4096);
    loop {
        buf.clear();
        match rx.recv_timeout(ping) {
            Ok(first) => {
                // Coalesce whatever is queued; consecutive moves are merged into one.
                let mut pending: Option<(i32, i32)> = None;
                let mut push = |m: Msg, buf: &mut Vec<u8>| match m {
                    Msg::Move { dx, dy } => {
                        let p = pending.get_or_insert((0, 0));
                        p.0 = p.0.saturating_add(dx);
                        p.1 = p.1.saturating_add(dy);
                    }
                    other => {
                        if let Some((dx, dy)) = pending.take() {
                            Msg::Move { dx, dy }.encode(buf);
                        }
                        other.encode(buf);
                    }
                };
                push(first, &mut buf);
                while let Ok(m) = rx.try_recv() {
                    push(m, &mut buf);
                }
                if let Some((dx, dy)) = pending.take() {
                    Msg::Move { dx, dy }.encode(&mut buf);
                }
            }
            Err(RecvTimeoutError::Timeout) => Msg::Ping.encode(&mut buf),
            Err(RecvTimeoutError::Disconnected) => return None,
        }
        if let Err(e) = s.write_all(&buf) {
            return Some(e.to_string());
        }
    }
}
