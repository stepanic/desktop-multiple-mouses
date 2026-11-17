//! HID usage -> PC scan code (set 1), which is what the wire protocol carries.

/// Keyboard page (0x07) usage -> (scancode, e0).
pub fn keyboard_usage_to_scancode(usage: u16) -> Option<(u16, bool)> {
    let n = |s| Some((s, false));
    let e = |s| Some((s, true));
    match usage {
        0x04 => n(0x1E), // A
        0x05 => n(0x30),
        0x06 => n(0x2E),
        0x07 => n(0x20),
        0x08 => n(0x12),
        0x09 => n(0x21),
        0x0A => n(0x22),
        0x0B => n(0x23),
        0x0C => n(0x17),
        0x0D => n(0x24),
        0x0E => n(0x25),
        0x0F => n(0x26),
        0x10 => n(0x32),
        0x11 => n(0x31),
        0x12 => n(0x18),
        0x13 => n(0x19),
        0x14 => n(0x10),
        0x15 => n(0x13),
        0x16 => n(0x1F),
        0x17 => n(0x14),
        0x18 => n(0x16),
        0x19 => n(0x2F),
        0x1A => n(0x11),
        0x1B => n(0x2D),
        0x1C => n(0x15),
        0x1D => n(0x2C), // Z
        0x1E..=0x26 => n(usage - 0x1E + 0x02), // 1..9
        0x27 => n(0x0B), // 0
        0x28 => n(0x1C), // Enter
        0x29 => n(0x01), // Esc
        0x2A => n(0x0E), // Backspace
        0x2B => n(0x0F), // Tab
        0x2C => n(0x39), // Space
        0x2D => n(0x0C), // -
        0x2E => n(0x0D), // =
        0x2F => n(0x1A), // [
        0x30 => n(0x1B), // ]
        0x31 => n(0x2B), // backslash
        0x32 => n(0x2B), // non-US #
        0x33 => n(0x27), // ;
        0x34 => n(0x28), // '
        0x35 => n(0x29), // `
        0x36 => n(0x33), // ,
        0x37 => n(0x34), // .
        0x38 => n(0x35), // /
        0x39 => n(0x3A), // Caps Lock
        0x3A..=0x43 => n(usage - 0x3A + 0x3B), // F1..F10
        0x44 => n(0x57), // F11
        0x45 => n(0x58), // F12
        0x46 => e(0x37), // Print Screen
        0x47 => n(0x46), // Scroll Lock
        0x49 => e(0x52), // Insert
        0x4A => e(0x47), // Home
        0x4B => e(0x49), // Page Up
        0x4C => e(0x53), // Delete
        0x4D => e(0x4F), // End
        0x4E => e(0x51), // Page Down
        0x4F => e(0x4D), // Right
        0x50 => e(0x4B), // Left
        0x51 => e(0x50), // Down
        0x52 => e(0x48), // Up
        0x53 => n(0x45), // Num Lock
        0x54 => e(0x35), // keypad /
        0x55 => n(0x37), // keypad *
        0x56 => n(0x4A), // keypad -
        0x57 => n(0x4E), // keypad +
        0x58 => e(0x1C), // keypad Enter
        0x59 => n(0x4F), // keypad 1
        0x5A => n(0x50),
        0x5B => n(0x51),
        0x5C => n(0x4B),
        0x5D => n(0x4C),
        0x5E => n(0x4D),
        0x5F => n(0x47),
        0x60 => n(0x48),
        0x61 => n(0x49), // keypad 9
        0x62 => n(0x52), // keypad 0
        0x63 => n(0x53), // keypad .
        0x64 => n(0x56), // ISO key next to left shift (<> on HR keyboards)
        0x65 => e(0x5D), // Application / Menu
        0x68 => n(0x64), // F13
        0x69 => n(0x65), // F14
        0x6A => n(0x66), // F15
        0x7F => e(0x20), // Mute
        0x80 => e(0x30), // Volume up
        0x81 => e(0x2E), // Volume down
        0xE0 => n(0x1D), // left Ctrl
        0xE1 => n(0x2A), // left Shift
        0xE2 => n(0x38), // left Alt
        0xE3 => e(0x5B), // left GUI (Windows)
        0xE4 => e(0x1D), // right Ctrl
        0xE5 => n(0x36), // right Shift
        0xE6 => e(0x38), // right Alt (AltGr)
        0xE7 => e(0x5C), // right GUI
        _ => None,
    }
}

/// Consumer page (0x0C) usage -> (scancode, e0) for the few keys with a PC equivalent.
pub fn consumer_usage_to_scancode(usage: u16) -> Option<(u16, bool)> {
    match usage {
        0xE2 => Some((0x20, true)), // Mute
        0xE9 => Some((0x30, true)), // Volume up
        0xEA => Some((0x2E, true)), // Volume down
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples() {
        assert_eq!(keyboard_usage_to_scancode(0x04), Some((0x1E, false)));
        assert_eq!(keyboard_usage_to_scancode(0x1E), Some((0x02, false)));
        assert_eq!(keyboard_usage_to_scancode(0x26), Some((0x0A, false)));
        assert_eq!(keyboard_usage_to_scancode(0x3A), Some((0x3B, false)));
        assert_eq!(keyboard_usage_to_scancode(0x43), Some((0x44, false)));
        assert_eq!(keyboard_usage_to_scancode(0x50), Some((0x4B, true)));
        assert_eq!(keyboard_usage_to_scancode(0xE3), Some((0x5B, true)));
    }
}
