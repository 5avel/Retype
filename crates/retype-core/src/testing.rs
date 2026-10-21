//! Test fixtures: hardcoded tables for the standard en-US, ru-RU and uk-UA layouts.

use crate::layout::{KeyPos, LayoutTable};

/// Scan codes of the four character rows of a PC keyboard, left to right.
const ROWS: [&[u16]; 5] = [
    &[
        0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
    ],
    &[
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B,
    ],
    &[
        0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28,
    ],
    &[0x2B],
    &[0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35],
];

/// `rows` holds (unshifted, shifted) strings for each entry of [`ROWS`].
fn table(rows: [(&str, &str); 5]) -> LayoutTable {
    let mut entries = Vec::new();
    for shift in [false, true] {
        for (scans, (lower, upper)) in ROWS.iter().zip(rows) {
            let chars = if shift { upper } else { lower };
            assert_eq!(chars.chars().count(), scans.len(), "fixture row {chars}");
            for (&scan, c) in scans.iter().zip(chars.chars()) {
                entries.push((KeyPos { scan, shift }, c));
            }
        }
    }
    LayoutTable::from_entries(entries)
}

pub fn en() -> LayoutTable {
    table([
        ("`1234567890-=", "~!@#$%^&*()_+"),
        ("qwertyuiop[]", "QWERTYUIOP{}"),
        ("asdfghjkl;'", "ASDFGHJKL:\""),
        ("\\", "|"),
        ("zxcvbnm,./", "ZXCVBNM<>?"),
    ])
}

pub fn ru() -> LayoutTable {
    table([
        ("ё1234567890-=", "Ё!\"№;%:?*()_+"),
        ("йцукенгшщзхъ", "ЙЦУКЕНГШЩЗХЪ"),
        ("фывапролджэ", "ФЫВАПРОЛДЖЭ"),
        ("\\", "/"),
        ("ячсмитьбю.", "ЯЧСМИТЬБЮ,"),
    ])
}

pub fn uk() -> LayoutTable {
    table([
        ("'1234567890-=", "₴!\"№;%:?*()_+"),
        ("йцукенгшщзхї", "ЙЦУКЕНГШЩЗХЇ"),
        ("фівапролджє", "ФІВАПРОЛДЖЄ"),
        ("ґ", "Ґ"),
        ("ячсмитьбю.", "ЯЧСМИТЬБЮ,"),
    ])
}
