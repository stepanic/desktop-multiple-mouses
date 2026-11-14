//! Minimal dynamic binding to interception.dll (https://github.com/oblitum/Interception).
//! The DLL is loaded at runtime so no import library is needed to build.

use libloading::Library;
use std::ffi::c_void;

pub const MAX_KEYBOARD: i32 = 10;
pub const MAX_MOUSE: i32 = 10;
pub const MAX_DEVICE: i32 = MAX_KEYBOARD + MAX_MOUSE;

pub const FILTER_NONE: u16 = 0x0000;
pub const FILTER_ALL: u16 = 0xFFFF;

pub const KEY_UP: u16 = 0x01;
pub const KEY_E0: u16 = 0x02;
pub const KEY_E1: u16 = 0x04;

pub const MOUSE_LEFT_DOWN: u16 = 0x001;
pub const MOUSE_LEFT_UP: u16 = 0x002;
pub const MOUSE_RIGHT_DOWN: u16 = 0x004;
pub const MOUSE_RIGHT_UP: u16 = 0x008;
pub const MOUSE_MIDDLE_DOWN: u16 = 0x010;
pub const MOUSE_MIDDLE_UP: u16 = 0x020;
pub const MOUSE_B4_DOWN: u16 = 0x040;
pub const MOUSE_B4_UP: u16 = 0x080;
pub const MOUSE_B5_DOWN: u16 = 0x100;
pub const MOUSE_B5_UP: u16 = 0x200;
pub const MOUSE_WHEEL: u16 = 0x400;
pub const MOUSE_HWHEEL: u16 = 0x800;

pub const MOUSE_FLAG_ABSOLUTE: u16 = 0x001;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct MouseStroke {
    pub state: u16,
    pub flags: u16,
    pub rolling: i16,
    pub x: i32,
    pub y: i32,
    pub information: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct KeyStroke {
    pub code: u16,
    pub state: u16,
    pub information: u32,
}

/// Same size/layout as `InterceptionStroke` (a char buffer the size of the mouse stroke).
#[repr(C)]
#[derive(Clone, Copy)]
pub union Stroke {
    pub mouse: MouseStroke,
    pub key: KeyStroke,
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke { mouse: MouseStroke::default() }
    }
}

pub type Predicate = unsafe extern "C" fn(device: i32) -> i32;

type FnCreateContext = unsafe extern "C" fn() -> *mut c_void;
type FnDestroyContext = unsafe extern "C" fn(*mut c_void);
type FnSetFilter = unsafe extern "C" fn(*mut c_void, Option<Predicate>, u16);
type FnWaitWithTimeout = unsafe extern "C" fn(*mut c_void, u32) -> i32;
type FnSend = unsafe extern "C" fn(*mut c_void, i32, *const Stroke, u32) -> i32;
type FnReceive = unsafe extern "C" fn(*mut c_void, i32, *mut Stroke, u32) -> i32;
type FnGetHardwareId = unsafe extern "C" fn(*mut c_void, i32, *mut c_void, u32) -> u32;

pub struct Interception {
    _lib: Library,
    ctx: *mut c_void,
    destroy: FnDestroyContext,
    set_filter: FnSetFilter,
    wait_with_timeout: FnWaitWithTimeout,
    send: FnSend,
    receive: FnReceive,
    get_hardware_id: FnGetHardwareId,
}

pub fn is_keyboard(device: i32) -> bool {
    (1..=MAX_KEYBOARD).contains(&device)
}

pub fn is_mouse(device: i32) -> bool {
    (MAX_KEYBOARD + 1..=MAX_DEVICE).contains(&device)
}

impl Interception {
    pub fn load() -> Result<Interception, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let candidates = exe_dir
            .map(|d| d.join("interception.dll"))
            .into_iter()
            .chain(std::iter::once("interception.dll".into()));
        let mut last_err = String::new();
        for path in candidates {
            match unsafe { Library::new(&path) } {
                Ok(lib) => return unsafe { Self::from_lib(lib) },
                Err(e) => last_err = format!("{}: {e}", path.display()),
            }
        }
        Err(format!(
            "Ne mogu učitati interception.dll ({last_err}).\n\
             Kopiraj library\\x64\\interception.dll iz Interception paketa pored multi-mouse.exe."
        ))
    }

    unsafe fn from_lib(lib: Library) -> Result<Interception, String> {
        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *lib.get::<$ty>($name)
                    .map_err(|e| format!("interception.dll nema funkciju {}: {e}", String::from_utf8_lossy($name)))?
            };
        }
        let create: FnCreateContext = sym!(b"interception_create_context\0", FnCreateContext);
        let destroy = sym!(b"interception_destroy_context\0", FnDestroyContext);
        let set_filter = sym!(b"interception_set_filter\0", FnSetFilter);
        let wait_with_timeout = sym!(b"interception_wait_with_timeout\0", FnWaitWithTimeout);
        let send = sym!(b"interception_send\0", FnSend);
        let receive = sym!(b"interception_receive\0", FnReceive);
        let get_hardware_id = sym!(b"interception_get_hardware_id\0", FnGetHardwareId);

        let ctx = create();
        if ctx.is_null() {
            return Err("Interception driver nije instaliran (ili nije napravljen restart nakon instalacije).\n\
                 Pokreni kao administrator: install-interception.exe /install  i zatim restartaj računalo."
                .into());
        }
        Ok(Interception {
            _lib: lib,
            ctx,
            destroy,
            set_filter,
            wait_with_timeout,
            send,
            receive,
            get_hardware_id,
        })
    }

    pub fn set_filter(&self, predicate: Predicate, filter: u16) {
        unsafe { (self.set_filter)(self.ctx, Some(predicate), filter) }
    }

    /// Returns the device that has input waiting, or `None` on timeout.
    pub fn wait(&self, timeout_ms: u32) -> Option<i32> {
        let d = unsafe { (self.wait_with_timeout)(self.ctx, timeout_ms) };
        (d > 0).then_some(d)
    }

    pub fn receive(&self, device: i32) -> Option<Stroke> {
        let mut s = Stroke::default();
        let n = unsafe { (self.receive)(self.ctx, device, &mut s, 1) };
        (n > 0).then_some(s)
    }

    pub fn send(&self, device: i32, stroke: &Stroke) {
        unsafe {
            (self.send)(self.ctx, device, stroke, 1);
        }
    }

    /// First string of the device's hardware-id multi-string, e.g.
    /// `HID\VID_046D&PID_C52B&REV_1211&MI_00`. `None` if no device is present.
    pub fn hardware_id(&self, device: i32) -> Option<String> {
        let mut buf = [0u16; 512];
        let bytes = unsafe {
            (self.get_hardware_id)(
                self.ctx,
                device,
                buf.as_mut_ptr() as *mut c_void,
                (buf.len() * 2) as u32,
            )
        };
        if bytes == 0 {
            return None;
        }
        let len = ((bytes as usize) / 2).min(buf.len());
        let end = buf[..len].iter().position(|&c| c == 0).unwrap_or(len);
        let s = String::from_utf16_lossy(&buf[..end]);
        (!s.is_empty()).then_some(s)
    }
}

impl Drop for Interception {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.ctx) }
    }
}
