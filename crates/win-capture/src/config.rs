use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    /// Mac agent address: Tailscale MagicDNS name or 100.x.y.z IP, optionally with :port.
    pub mac_address: String,
    /// Token printed by `multi-mouse-agent` on the Mac.
    pub token: String,
    /// Hardware IDs of keyboards whose input goes to the Mac.
    pub keyboard_ids: Vec<String>,
    /// Hardware IDs of mice whose input goes to the Mac.
    pub mouse_ids: Vec<String>,
}

impl Config {
    pub fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("multi-mouse.toml")))
            .unwrap_or_else(|| PathBuf::from("multi-mouse.toml"))
    }

    pub fn load() -> Result<Option<Config>, String> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(s) => toml::from_str(&s)
                .map(Some)
                .map_err(|e| format!("Greška u {}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("Ne mogu pročitati {}: {e}", path.display())),
        }
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = Self::path();
        let s = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, s).map_err(|e| format!("Ne mogu spremiti {}: {e}", path.display()))?;
        Ok(path)
    }

    /// `mac_address` with the default port appended when missing.
    pub fn socket_address(&self) -> String {
        let a = self.mac_address.trim();
        let has_port = if a.starts_with('[') {
            a.contains("]:")
        } else {
            a.matches(':').count() == 1
        };
        if has_port {
            a.to_string()
        } else if a.contains(':') {
            format!("[{a}]:{}", mm_proto::DEFAULT_PORT)
        } else {
            format!("{a}:{}", mm_proto::DEFAULT_PORT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(addr: &str) -> Config {
        Config { mac_address: addr.into(), token: String::new(), keyboard_ids: vec![], mouse_ids: vec![] }
    }

    #[test]
    fn address_port() {
        assert_eq!(cfg("macbook").socket_address(), "macbook:47800");
        assert_eq!(cfg("100.1.2.3:9000").socket_address(), "100.1.2.3:9000");
        assert_eq!(cfg("fd7a::1").socket_address(), "[fd7a::1]:47800");
        assert_eq!(cfg("[fd7a::1]:5").socket_address(), "[fd7a::1]:5");
    }
}
