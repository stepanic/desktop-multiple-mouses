//! PC scan code (set 1) -> macOS virtual key code (kVK_*), by physical key position.

pub fn scancode_to_mac(scancode: u16, e0: bool) -> Option<u16> {
    let k = if e0 {
        match scancode {
            0x1C => 76,  // keypad Enter
            0x1D => 62,  // right Ctrl
            0x35 => 75,  // keypad /
            0x37 => 105, // Print Screen -> F13
            0x38 => 61,  // right Alt / AltGr -> right Option
            0x47 => 115, // Home
            0x48 => 126, // Up
            0x49 => 116, // Page Up
            0x4B => 123, // Left
            0x4D => 124, // Right
            0x4F => 119, // End
            0x50 => 125, // Down
            0x51 => 121, // Page Down
            0x52 => 114, // Insert -> Help
            0x53 => 117, // Delete (forward)
            0x5B => 55,  // left Windows -> left Command
            0x5C => 54,  // right Windows -> right Command
            0x5D => 110, // Menu -> context menu key
            0x20 => 74,  // Mute
            0x2E => 73,  // Volume down
            0x30 => 72,  // Volume up
            _ => return None,
        }
    } else {
        match scancode {
            0x01 => 53, // Esc
            0x02 => 18, // 1
            0x03 => 19,
            0x04 => 20,
            0x05 => 21,
            0x06 => 23,
            0x07 => 22,
            0x08 => 26,
            0x09 => 28,
            0x0A => 25,
            0x0B => 29, // 0
            0x0C => 27, // -
            0x0D => 24, // =
            0x0E => 51, // Backspace
            0x0F => 48, // Tab
            0x10 => 12, // Q
            0x11 => 13, // W
            0x12 => 14, // E
            0x13 => 15, // R
            0x14 => 17, // T
            0x15 => 16, // Y (Z on QWERTZ - layout is applied on the Mac)
            0x16 => 32, // U
            0x17 => 34, // I
            0x18 => 31, // O
            0x19 => 35, // P
            0x1A => 33, // [
            0x1B => 30, // ]
            0x1C => 36, // Enter
            0x1D => 59, // left Ctrl
            0x1E => 0,  // A
            0x1F => 1,  // S
            0x20 => 2,  // D
            0x21 => 3,  // F
            0x22 => 5,  // G
            0x23 => 4,  // H
            0x24 => 38, // J
            0x25 => 40, // K
            0x26 => 37, // L
            0x27 => 41, // ;
            0x28 => 39, // '
            0x29 => 50, // ` (key left of 1)
            0x2A => 56, // left Shift
            0x2B => 42, // backslash
            0x2C => 6,  // Z
            0x2D => 7,  // X
            0x2E => 8,  // C
            0x2F => 9,  // V
            0x30 => 11, // B
            0x31 => 45, // N
            0x32 => 46, // M
            0x33 => 43, // ,
            0x34 => 47, // .
            0x35 => 44, // /
            0x36 => 60, // right Shift
            0x37 => 67, // keypad *
            0x38 => 58, // left Alt -> left Option
            0x39 => 49, // Space
            0x3A => 57, // Caps Lock
            0x3B => 122, // F1
            0x3C => 120, // F2
            0x3D => 99,  // F3
            0x3E => 118, // F4
            0x3F => 96,  // F5
            0x40 => 97,  // F6
            0x41 => 98,  // F7
            0x42 => 100, // F8
            0x43 => 101, // F9
            0x44 => 109, // F10
            0x45 => 71,  // Num Lock -> keypad Clear
            0x46 => 107, // Scroll Lock -> F14
            0x47 => 89,  // keypad 7
            0x48 => 91,  // keypad 8
            0x49 => 92,  // keypad 9
            0x4A => 78,  // keypad -
            0x4B => 86,  // keypad 4
            0x4C => 87,  // keypad 5
            0x4D => 88,  // keypad 6
            0x4E => 69,  // keypad +
            0x4F => 83,  // keypad 1
            0x50 => 84,  // keypad 2
            0x51 => 85,  // keypad 3
            0x52 => 82,  // keypad 0
            0x53 => 65,  // keypad .
            0x56 => 10,  // ISO key left of Z (<> on HR/DE keyboards)
            0x57 => 103, // F11
            0x58 => 111, // F12
            0x64 => 105, // F13
            0x65 => 107, // F14
            0x66 => 113, // F15
            _ => return None,
        }
    };
    Some(k)
}

pub const KC_LSHIFT: u16 = 56;
pub const KC_RSHIFT: u16 = 60;
pub const KC_LCTRL: u16 = 59;
pub const KC_RCTRL: u16 = 62;
pub const KC_LOPT: u16 = 58;
pub const KC_ROPT: u16 = 61;
pub const KC_LCMD: u16 = 55;
pub const KC_RCMD: u16 = 54;
pub const KC_CAPS: u16 = 57;
pub const KC_GRAVE: u16 = 50;
pub const KC_ISO_SECTION: u16 = 10;

const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
pub const FLAG_ALPHA_SHIFT: u64 = 0x0001_0000;

/// CGEventFlags contributed by a held modifier key (device-independent + left/right bit),
/// or `None` if the key is not a modifier.
pub fn modifier_flags(keycode: u16) -> Option<u64> {
    Some(match keycode {
        KC_LCTRL => FLAG_CONTROL | 0x0001,
        KC_RCTRL => FLAG_CONTROL | 0x2000,
        KC_LSHIFT => FLAG_SHIFT | 0x0002,
        KC_RSHIFT => FLAG_SHIFT | 0x0004,
        KC_LCMD => FLAG_COMMAND | 0x0008,
        KC_RCMD => FLAG_COMMAND | 0x0010,
        KC_LOPT => FLAG_ALTERNATE | 0x0020,
        KC_ROPT => FLAG_ALTERNATE | 0x0040,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_and_arrows() {
        assert_eq!(scancode_to_mac(0x1E, false), Some(0)); // A
        assert_eq!(scancode_to_mac(0x4B, true), Some(123)); // Left arrow
        assert_eq!(scancode_to_mac(0x4B, false), Some(86)); // keypad 4
        assert_eq!(scancode_to_mac(0x5B, true), Some(KC_LCMD));
        assert!(modifier_flags(KC_LSHIFT).is_some());
        assert!(modifier_flags(0).is_none());
    }
}
