//! Turns successive HID reports (absolute "what is pressed now" snapshots) into
//! press/release/move events of the wire protocol.

use crate::hid::{Descriptor, Report};
use crate::usbkeys::{consumer_usage_to_scancode, keyboard_usage_to_scancode};
use mm_proto::{Button, Msg};
use std::collections::{BTreeSet, HashMap};

const WHEEL_NOTCH: i32 = 120;

#[derive(Default)]
struct PrevState {
    keys: BTreeSet<u16>,
    consumer: BTreeSet<u16>,
    buttons: u32,
}

pub struct Translator {
    desc: Descriptor,
    prev: HashMap<u8, PrevState>,
}

impl Translator {
    pub fn new(desc: Descriptor) -> Translator {
        Translator { desc, prev: HashMap::new() }
    }

    pub fn feed(&mut self, report: &[u8], out: &mut Vec<Msg>) {
        let Some(r) = self.desc.decode(report) else { return };
        let prev = self.prev.entry(r.report_id).or_default();
        translate(prev, &r, out);
    }

    /// Events that release everything this device currently holds.
    pub fn release_all(&mut self, out: &mut Vec<Msg>) {
        for prev in self.prev.values_mut() {
            let empty = Report { has_keys: true, has_consumer: true, buttons: Some(0), ..Default::default() };
            translate(prev, &empty, out);
        }
    }
}

fn translate(prev: &mut PrevState, r: &Report, out: &mut Vec<Msg>) {
    if r.has_keys && !r.rollover {
        diff(&prev.keys, &r.keys, keyboard_usage_to_scancode, out);
        prev.keys = r.keys.clone();
    }
    if r.has_consumer {
        diff(&prev.consumer, &r.consumer, consumer_usage_to_scancode, out);
        prev.consumer = r.consumer.clone();
    }
    if let Some(buttons) = r.buttons {
        let changed = buttons ^ prev.buttons;
        for (bit, button) in [(0, Button::Left), (1, Button::Right), (2, Button::Middle), (3, Button::Back), (4, Button::Forward)] {
            if changed & (1 << bit) != 0 {
                out.push(Msg::Button { button, down: buttons & (1 << bit) != 0 });
            }
        }
        prev.buttons = buttons;
    }
    if r.dx != 0 || r.dy != 0 {
        out.push(Msg::Move { dx: r.dx, dy: r.dy });
    }
    if r.wheel != 0 || r.hwheel != 0 {
        out.push(Msg::Wheel { vertical: r.wheel * WHEEL_NOTCH, horizontal: r.hwheel * WHEEL_NOTCH });
    }
}

fn diff(prev: &BTreeSet<u16>, now: &BTreeSet<u16>, map: fn(u16) -> Option<(u16, bool)>, out: &mut Vec<Msg>) {
    // Releases first, then presses (so e.g. a quick key change never leaves both "down").
    for &u in prev.difference(now) {
        if let Some((scancode, e0)) = map(u) {
            out.push(Msg::Key { scancode, e0, down: false });
        }
    }
    // Modifiers (0xE0..) before regular keys so "Shift+A" arriving in one report works.
    let mut pressed: Vec<u16> = now.difference(prev).copied().collect();
    pressed.sort_by_key(|&u| if u >= 0xE0 { 0 } else { 1 });
    for u in pressed {
        if let Some((scancode, e0)) = map(u) {
            out.push(Msg::Key { scancode, e0, down: true });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYBOARD: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75,
        0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00,
        0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xC0,
    ];

    #[test]
    fn shift_a_then_release() {
        let mut t = Translator::new(Descriptor::parse(KEYBOARD));
        let mut out = Vec::new();
        t.feed(&[0x02, 0, 0x04, 0, 0, 0, 0, 0], &mut out);
        assert_eq!(
            out,
            vec![
                Msg::Key { scancode: 0x2A, e0: false, down: true },
                Msg::Key { scancode: 0x1E, e0: false, down: true },
            ]
        );
        out.clear();
        t.feed(&[0, 0, 0, 0, 0, 0, 0, 0], &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|m| matches!(m, Msg::Key { down: false, .. })));
    }

    #[test]
    fn release_all_releases_held() {
        let mut t = Translator::new(Descriptor::parse(KEYBOARD));
        let mut out = Vec::new();
        t.feed(&[0, 0, 0x05, 0, 0, 0, 0, 0], &mut out);
        out.clear();
        t.release_all(&mut out);
        assert_eq!(out, vec![Msg::Key { scancode: 0x30, e0: false, down: false }]);
    }
}
