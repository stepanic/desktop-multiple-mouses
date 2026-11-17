//! Delivers events to Windows itself (used in "Windows mode", toggled with Ctrl+Alt+F12).

use mm_proto::{Button, Msg};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

fn send(input: INPUT) {
    unsafe {
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

fn mouse(dx: i32, dy: i32, data: i32, flags: u32) {
    send(INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as _, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    });
}

pub fn to_windows(m: Msg) {
    match m {
        Msg::Key { scancode, e0, down } => {
            let mut flags = KEYEVENTF_SCANCODE;
            if e0 {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            if !down {
                flags |= KEYEVENTF_KEYUP;
            }
            send(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: 0, wScan: scancode, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
            });
        }
        Msg::Move { dx, dy } => mouse(dx, dy, 0, MOUSEEVENTF_MOVE),
        Msg::Button { button, down } => {
            let (flags, data) = match (button, down) {
                (Button::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                (Button::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
                (Button::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                (Button::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
                (Button::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                (Button::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                (Button::Back, true) => (MOUSEEVENTF_XDOWN, 1),
                (Button::Back, false) => (MOUSEEVENTF_XUP, 1),
                (Button::Forward, true) => (MOUSEEVENTF_XDOWN, 2),
                (Button::Forward, false) => (MOUSEEVENTF_XUP, 2),
            };
            mouse(0, 0, data, flags);
        }
        Msg::Wheel { vertical, horizontal } => {
            if vertical != 0 {
                mouse(0, 0, vertical, MOUSEEVENTF_WHEEL);
            }
            if horizontal != 0 {
                mouse(0, 0, horizontal, MOUSEEVENTF_HWHEEL);
            }
        }
        Msg::ReleaseAll | Msg::Ping => {}
    }
}
