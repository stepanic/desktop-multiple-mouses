//! Reads a USB HID device (keyboard/mouse receiver) directly through libusb.
//! The device must use the WinUSB driver (installed with Zadig), so Windows itself
//! no longer treats it as a keyboard/mouse.

use crate::hid::Descriptor;
use crate::translate::Translator;
use mm_proto::Msg;
use rusb::{Direction, GlobalContext, Recipient, RequestType, TransferType, UsbContext};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HidInterface {
    pub number: u8,
    pub endpoint: u8,
    pub max_packet: usize,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    pub interfaces: Vec<HidInterface>,
    pub openable: bool,
}

fn hid_interfaces(dev: &rusb::Device<GlobalContext>) -> Vec<HidInterface> {
    let Ok(cfg) = dev.active_config_descriptor().or_else(|_| dev.config_descriptor(0)) else { return vec![] };
    let mut out = Vec::new();
    for iface in cfg.interfaces() {
        for alt in iface.descriptors().filter(|a| a.setting_number() == 0 && a.class_code() == 3) {
            if let Some(ep) = alt
                .endpoint_descriptors()
                .find(|e| e.direction() == Direction::In && e.transfer_type() == TransferType::Interrupt)
            {
                out.push(HidInterface {
                    number: alt.interface_number(),
                    endpoint: ep.address(),
                    max_packet: ep.max_packet_size() as usize,
                });
            }
        }
    }
    out
}

mod registry {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn subkeys(path: &str) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let mut key: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, wide(path).as_ptr(), 0, KEY_READ, &mut key) != ERROR_SUCCESS {
                return out;
            }
            for i in 0.. {
                let mut name = [0u16; 256];
                let mut len = name.len() as u32;
                let r = RegEnumKeyExW(key, i, name.as_mut_ptr(), &mut len, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
                if r != ERROR_SUCCESS {
                    break;
                }
                out.push(String::from_utf16_lossy(&name[..len as usize]));
            }
            RegCloseKey(key);
        }
        out
    }

    pub fn string_value(path: &str, name: &str) -> Option<String> {
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                wide(path).as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut _,
                &mut size,
            )
        };
        if r != ERROR_SUCCESS {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

/// Windows driver services bound to each interface of the device, from
/// HKLM\SYSTEM\CurrentControlSet\Enum\USB. Key: interface number (None = non-composite device).
pub fn interface_services(vid: u16, pid: u16) -> Vec<(Option<u8>, String)> {
    const BASE: &str = r"SYSTEM\CurrentControlSet\Enum\USB";
    let prefix = format!("VID_{vid:04X}&PID_{pid:04X}");
    let mut out = Vec::new();
    for k in registry::subkeys(BASE) {
        let up = k.to_uppercase();
        let Some(rest) = up.strip_prefix(&prefix) else { continue };
        let mi = if rest.is_empty() {
            None
        } else if let Some(h) = rest.strip_prefix("&MI_") {
            u8::from_str_radix(h.get(..2).unwrap_or(""), 16).ok()
        } else {
            continue;
        };
        for inst in registry::subkeys(&format!(r"{BASE}\{k}")) {
            if let Some(svc) = registry::string_value(&format!(r"{BASE}\{k}\{inst}"), "Service") {
                out.push((mi, svc));
            }
        }
    }
    out
}

/// True if the given interface (or the whole device) has the WinUSB driver.
pub fn has_winusb(services: &[(Option<u8>, String)], iface: u8) -> bool {
    services
        .iter()
        .any(|(mi, svc)| (*mi == Some(iface) || mi.is_none()) && svc.eq_ignore_ascii_case("WinUSB"))
}

/// All USB devices with HID interfaces. `openable` = every HID interface has the WinUSB driver.
pub fn list_candidates() -> Vec<Candidate> {
    let Ok(devices) = rusb::devices() else { return vec![] };
    let mut out = Vec::new();
    for dev in devices.iter() {
        let Ok(dd) = dev.device_descriptor() else { continue };
        let interfaces = hid_interfaces(&dev);
        if interfaces.is_empty() {
            continue;
        }
        let name = dev
            .open()
            .ok()
            .and_then(|h| h.read_product_string_ascii(&dd).ok())
            .unwrap_or_default();
        let services = interface_services(dd.vendor_id(), dd.product_id());
        let openable = interfaces.iter().all(|i| has_winusb(&services, i.number));
        out.push(Candidate { vid: dd.vendor_id(), pid: dd.product_id(), name, interfaces, openable });
    }
    out
}

fn report_descriptor(h: &rusb::DeviceHandle<GlobalContext>, iface: u8) -> rusb::Result<Vec<u8>> {
    let mut buf = vec![0u8; 4096];
    let rt = rusb::request_type(Direction::In, RequestType::Standard, Recipient::Interface);
    let n = h.read_control(rt, 0x06, 0x2200, iface as u16, &mut buf, Duration::from_secs(1))?;
    buf.truncate(n);
    Ok(buf)
}

/// Opens the device and streams its events to `tx` until it disappears or `stop` is set.
/// Returns a human-readable reason.
pub fn run_device(vid: u16, pid: u16, tx: &Sender<Msg>, stop: &AtomicBool) -> String {
    let ctx = GlobalContext::default();
    let Some(dev) = ctx
        .devices()
        .ok()
        .and_then(|l| l.iter().find(|d| d.device_descriptor().is_ok_and(|dd| dd.vendor_id() == vid && dd.product_id() == pid)))
    else {
        return "uređaj nije spojen".into();
    };
    let interfaces = hid_interfaces(&dev);
    let handle = match dev.open() {
        Ok(h) => h,
        Err(rusb::Error::NotSupported) | Err(rusb::Error::Access) | Err(rusb::Error::NotFound) => {
            return "ne mogu otvoriti uređaj - je li WinUSB driver instaliran (Zadig)?".into()
        }
        Err(e) => return format!("ne mogu otvoriti uređaj: {e}"),
    };

    let services = interface_services(vid, pid);
    let mut readers = Vec::new();
    for i in &interfaces {
        if !has_winusb(&services, i.number) {
            continue; // still owned by Windows' HID driver; libusb could only "half" open it
        }
        if handle.claim_interface(i.number).is_err() {
            continue; // interface still owned by Windows' HID driver; skip it
        }
        // SET_IDLE(0): only report on change.
        let rt = rusb::request_type(Direction::Out, RequestType::Class, Recipient::Interface);
        let _ = handle.write_control(rt, 0x0A, 0, i.number as u16, &[], Duration::from_millis(200));
        match report_descriptor(&handle, i.number) {
            Ok(d) => {
                let desc = Descriptor::parse(&d);
                if desc.has_input() {
                    readers.push((i.clone(), Translator::new(desc)));
                }
            }
            Err(e) => println!("[usb] sučelje {}: ne mogu pročitati HID descriptor ({e})", i.number),
        }
    }
    if readers.is_empty() {
        return "nijedno HID sučelje nije dostupno - je li WinUSB instaliran na Interface 0 i 1 (Zadig)?".into();
    }
    let list: Vec<String> = readers.iter().map(|(i, _)| i.number.to_string()).collect();
    println!("[usb] Čitam {:04x}:{:04x}, sučelja {}.", vid, pid, list.join(", "));

    let failed = AtomicBool::new(false);
    let reason = std::sync::Mutex::new(String::new());
    std::thread::scope(|s| {
        for (iface, mut tr) in readers {
            let (handle, tx, failed, reason) = (&handle, tx.clone(), &failed, &reason);
            s.spawn(move || {
                let mut buf = vec![0u8; iface.max_packet.max(64)];
                let mut out = Vec::new();
                while !failed.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                    match handle.read_interrupt(iface.endpoint, &mut buf, Duration::from_millis(200)) {
                        Ok(n) => {
                            tr.feed(&buf[..n], &mut out);
                            for m in out.drain(..) {
                                let _ = tx.send(m);
                            }
                        }
                        Err(rusb::Error::Timeout) | Err(rusb::Error::Interrupted) | Err(rusb::Error::Overflow) => {}
                        Err(e) => {
                            *reason.lock().unwrap() = format!("greška čitanja ({e})");
                            failed.store(true, Ordering::Relaxed);
                        }
                    }
                }
                tr.release_all(&mut out);
                for m in out.drain(..) {
                    let _ = tx.send(m);
                }
            });
        }
    });
    let r = reason.into_inner().unwrap();
    if r.is_empty() { "zaustavljeno".into() } else { r }
}
