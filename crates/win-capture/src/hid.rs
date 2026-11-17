//! Small HID report-descriptor parser and report decoder for keyboards and mice.
//! Platform independent so it can be unit-tested anywhere.

use std::collections::{BTreeSet, HashMap};

pub const PAGE_GENERIC_DESKTOP: u16 = 0x01;
pub const PAGE_KEYBOARD: u16 = 0x07;
pub const PAGE_BUTTON: u16 = 0x09;
pub const PAGE_CONSUMER: u16 = 0x0C;

const GD_X: u16 = 0x30;
const GD_Y: u16 = 0x31;
const GD_WHEEL: u16 = 0x38;
const CONSUMER_AC_PAN: u16 = 0x238;

#[derive(Debug, Clone)]
struct Field {
    report_id: u8,
    bit_offset: usize,
    bit_size: usize,
    count: usize,
    variable: bool,
    constant: bool,
    logical_min: i32,
    logical_max: i32,
    /// Explicit usages (page << 16 | id), in declaration order.
    usages: Vec<u32>,
    /// Usage range (page << 16 | id) when given with Usage Minimum/Maximum.
    usage_range: Option<(u32, u32)>,
}

impl Field {
    fn usage_for_index(&self, i: usize) -> Option<u32> {
        if let Some((min, max)) = self.usage_range {
            let u = min.checked_add(i as u32)?;
            return (u <= max).then_some(u);
        }
        if self.usages.is_empty() {
            return None;
        }
        Some(self.usages[i.min(self.usages.len() - 1)])
    }
}

#[derive(Debug, Clone, Default)]
pub struct Descriptor {
    fields: Vec<Field>,
    uses_report_ids: bool,
}

fn sign_extend(v: u32, size_bytes: usize) -> i32 {
    match size_bytes {
        1 => v as u8 as i8 as i32,
        2 => v as u16 as i16 as i32,
        _ => v as i32,
    }
}

impl Descriptor {
    pub fn parse(d: &[u8]) -> Descriptor {
        #[derive(Clone, Default)]
        struct Globals {
            page: u16,
            lmin: i32,
            lmax: i32,
            size: usize,
            count: usize,
            report_id: u8,
        }
        let mut g = Globals::default();
        let mut stack: Vec<Globals> = Vec::new();
        let mut usages: Vec<u32> = Vec::new();
        let mut umin: Option<u32> = None;
        let mut umax: Option<u32> = None;
        let mut offsets: HashMap<u8, usize> = HashMap::new();
        let mut out = Descriptor::default();

        let mut i = 0;
        while i < d.len() {
            let prefix = d[i];
            if prefix == 0xFE {
                // Long item: 0xFE, size, tag, data...
                let size = *d.get(i + 1).unwrap_or(&0) as usize;
                i += 3 + size;
                continue;
            }
            let size = match prefix & 3 {
                3 => 4,
                n => n as usize,
            };
            let ty = (prefix >> 2) & 3;
            let tag = prefix >> 4;
            let mut raw: u32 = 0;
            for b in 0..size {
                raw |= (*d.get(i + 1 + b).unwrap_or(&0) as u32) << (8 * b);
            }
            i += 1 + size;

            // Extended (4-byte) usages carry their own page.
            let full_usage = |raw: u32, page: u16| if size == 4 { raw } else { ((page as u32) << 16) | raw };

            match (ty, tag) {
                // Main items
                (0, 8) => {
                    let flags = raw;
                    let off = offsets.entry(g.report_id).or_insert(0);
                    let mut lmax = g.lmax;
                    if lmax < g.lmin {
                        lmax = (lmax as u32) as i32; // max was meant unsigned
                    }
                    let field = Field {
                        report_id: g.report_id,
                        bit_offset: *off,
                        bit_size: g.size,
                        count: g.count,
                        constant: flags & 1 != 0,
                        variable: flags & 2 != 0,
                        logical_min: g.lmin,
                        logical_max: lmax,
                        usages: std::mem::take(&mut usages),
                        usage_range: match (umin.take(), umax.take()) {
                            (Some(a), Some(b)) => Some((a, b)),
                            _ => None,
                        },
                    };
                    *off += g.size * g.count;
                    out.fields.push(field);
                }
                (0, 9) | (0, 11) => {
                    // Output / Feature: different report spaces; just reset locals.
                    usages.clear();
                    umin = None;
                    umax = None;
                }
                (0, _) => {
                    // Collection / End Collection
                    usages.clear();
                    umin = None;
                    umax = None;
                }
                // Global items
                (1, 0) => g.page = raw as u16,
                (1, 1) => g.lmin = sign_extend(raw, size),
                (1, 2) => g.lmax = sign_extend(raw, size),
                (1, 7) => g.size = raw as usize,
                (1, 8) => {
                    g.report_id = raw as u8;
                    out.uses_report_ids = true;
                }
                (1, 9) => g.count = raw as usize,
                (1, 10) => stack.push(g.clone()),
                (1, 11) => {
                    if let Some(s) = stack.pop() {
                        g = s;
                    }
                }
                // Local items
                (2, 0) => usages.push(full_usage(raw, g.page)),
                (2, 1) => umin = Some(full_usage(raw, g.page)),
                (2, 2) => umax = Some(full_usage(raw, g.page)),
                _ => {}
            }
        }
        out
    }

    pub fn has_input(&self) -> bool {
        self.fields.iter().any(|f| !f.constant)
    }
}

fn extract(data: &[u8], bit_offset: usize, bit_size: usize, signed: bool) -> Option<i32> {
    if bit_size == 0 || bit_size > 32 || bit_offset + bit_size > data.len() * 8 {
        return None;
    }
    let mut v: u64 = 0;
    for b in 0..bit_size {
        let bit = bit_offset + b;
        if data[bit / 8] & (1 << (bit % 8)) != 0 {
            v |= 1 << b;
        }
    }
    if signed && bit_size < 32 && v & (1 << (bit_size - 1)) != 0 {
        v |= !0u64 << bit_size;
    }
    Some(v as i64 as i32)
}

/// Everything one report says about the current input state.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub report_id: u8,
    /// Pressed keyboard-page usages (0xE0..0xE7 modifiers included).
    pub keys: BTreeSet<u16>,
    /// Pressed consumer-page usages (volume etc.).
    pub consumer: BTreeSet<u16>,
    /// Bit n = button n+1. `None` if this report has no buttons.
    pub buttons: Option<u32>,
    pub dx: i32,
    pub dy: i32,
    pub wheel: i32,
    pub hwheel: i32,
    /// True if the report contains keyboard fields (so an empty `keys` means "all released").
    pub has_keys: bool,
    pub has_consumer: bool,
    /// Keyboard reported ErrorRollOver: ignore the key state of this report.
    pub rollover: bool,
}

impl Descriptor {
    pub fn decode(&self, report: &[u8]) -> Option<Report> {
        let (id, data) = if self.uses_report_ids {
            (*report.first()?, &report[1..])
        } else {
            (0, report)
        };
        let mut r = Report { report_id: id, ..Default::default() };
        let mut any = false;
        for f in self.fields.iter().filter(|f| f.report_id == id && !f.constant) {
            any = true;
            let signed = f.logical_min < 0;
            for i in 0..f.count {
                let Some(v) = extract(data, f.bit_offset + i * f.bit_size, f.bit_size, signed) else { continue };
                if f.variable {
                    let Some(u) = f.usage_for_index(i) else { continue };
                    apply_variable(&mut r, u, v);
                } else {
                    // Array: value selects a usage; out-of-range = nothing pressed.
                    if v < f.logical_min || v > f.logical_max {
                        if let Some(u) = f.usage_for_index(0) {
                            mark_array_page(&mut r, (u >> 16) as u16);
                        }
                        continue;
                    }
                    let Some(u) = f.usage_for_index((v - f.logical_min) as usize) else { continue };
                    mark_array_page(&mut r, (u >> 16) as u16);
                    let (page, id) = ((u >> 16) as u16, u as u16);
                    match page {
                        PAGE_KEYBOARD if id == 0x01 || id == 0x02 || id == 0x03 => r.rollover |= id == 0x01,
                        PAGE_KEYBOARD if id != 0 => {
                            r.keys.insert(id);
                        }
                        PAGE_CONSUMER if id != 0 => {
                            r.consumer.insert(id);
                        }
                        PAGE_BUTTON if id != 0 && id <= 32 => {
                            *r.buttons.get_or_insert(0) |= 1 << (id - 1);
                        }
                        _ => {}
                    }
                }
            }
        }
        any.then_some(r)
    }
}

fn mark_array_page(r: &mut Report, page: u16) {
    match page {
        PAGE_KEYBOARD => r.has_keys = true,
        PAGE_CONSUMER => r.has_consumer = true,
        PAGE_BUTTON => {
            r.buttons.get_or_insert(0);
        }
        _ => {}
    }
}

fn apply_variable(r: &mut Report, usage: u32, v: i32) {
    let (page, id) = ((usage >> 16) as u16, usage as u16);
    match page {
        PAGE_KEYBOARD => {
            r.has_keys = true;
            if v != 0 && id >= 4 {
                r.keys.insert(id);
            }
        }
        PAGE_BUTTON => {
            let b = r.buttons.get_or_insert(0);
            if v != 0 && (1..=32).contains(&id) {
                *b |= 1 << (id - 1);
            }
        }
        PAGE_GENERIC_DESKTOP => match id {
            GD_X => r.dx += v,
            GD_Y => r.dy += v,
            GD_WHEEL => r.wheel += v,
            _ => {}
        },
        PAGE_CONSUMER => {
            if id == CONSUMER_AC_PAN {
                r.hwheel += v;
            } else {
                r.has_consumer = true;
                if v != 0 {
                    r.consumer.insert(id);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard boot keyboard descriptor (HID spec appendix B.1, input part).
    const KEYBOARD: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, // GD, Keyboard, Collection(Application)
        0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, // modifiers
        0x95, 0x01, 0x75, 0x08, 0x81, 0x01, // reserved byte
        0x95, 0x05, 0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, // LEDs (output)
        0x95, 0x01, 0x75, 0x03, 0x91, 0x01, // LED padding
        0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, // keys
        0xC0,
    ];

    /// Logitech-style mouse with report ID 2, 16 buttons, 12-bit X/Y, wheel and AC pan.
    const MOUSE: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x02, 0x09, 0x01, 0xA1, 0x00,
        0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x15, 0x00, 0x25, 0x01, 0x95, 0x10, 0x75, 0x01, 0x81, 0x02,
        0x05, 0x01, 0x16, 0x01, 0xF8, 0x26, 0xFF, 0x07, 0x75, 0x0C, 0x95, 0x02, 0x09, 0x30, 0x09, 0x31, 0x81, 0x06,
        0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95, 0x01, 0x09, 0x38, 0x81, 0x06,
        0x05, 0x0C, 0x0A, 0x38, 0x02, 0x95, 0x01, 0x81, 0x06,
        0xC0, 0xC0,
    ];

    #[test]
    fn keyboard_report() {
        let d = Descriptor::parse(KEYBOARD);
        // Left shift + 'a' + 'b'
        let r = d.decode(&[0x02, 0, 0x04, 0x05, 0, 0, 0, 0]).unwrap();
        assert!(r.has_keys);
        assert_eq!(r.keys.iter().copied().collect::<Vec<_>>(), vec![0x04, 0x05, 0xE1]);
        let r = d.decode(&[0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(r.has_keys && r.keys.is_empty());
        let r = d.decode(&[0, 0, 1, 1, 1, 1, 1, 1]).unwrap();
        assert!(r.rollover);
    }

    #[test]
    fn mouse_report() {
        let d = Descriptor::parse(MOUSE);
        // id 2, buttons: left+right (0x0003), X=-1 (0xFFF), Y=+5, wheel=-1, pan=+1
        let x: u32 = 0xFFF;
        let y: u32 = 5;
        let xy = x | (y << 12);
        let r = d
            .decode(&[0x02, 0x03, 0x00, xy as u8, (xy >> 8) as u8, (xy >> 16) as u8, 0xFF, 0x01])
            .unwrap();
        assert_eq!(r.buttons, Some(0b11));
        assert_eq!((r.dx, r.dy, r.wheel, r.hwheel), (-1, 5, -1, 1));
        assert!(d.decode(&[0x07, 0, 0]).is_none(), "unknown report id");
    }
}
