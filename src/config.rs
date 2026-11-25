pub const GRID_OPTIONS: [(&str, i32); 2] = [("40 (coarse)", 40), ("20 (medium)", 20)];
pub const DEFAULT_GRID: i32 = 20;
pub const MIN_BLOCK_WIDTH: i32 = 150;
pub const MIN_BLOCK_HEIGHT: i32 = 90;
pub const GRID_STEP: i32 = 10;
pub const PORT_HEIGHT: i32 = 20;

// Colour table from config.py, as (r, g, b).
pub const SIGNAL: (u8, u8, u8) = (0x4C, 0xAF, 0x50);
pub const UNDECLARED: (u8, u8, u8) = (0x60, 0x7D, 0x8B);
pub const HIGHLIGHT: (u8, u8, u8) = (0xFF, 0x6F, 0x00);
pub const INPUT_PORT: (u8, u8, u8) = (0x21, 0x96, 0xF3);
pub const OUTPUT_PORT: (u8, u8, u8) = (0xF4, 0x43, 0x36);
pub const INSTANCE_BG: (u8, u8, u8) = (0xE8, 0xF4, 0xF8);
pub const CLOCK: (u8, u8, u8) = (0x8E, 0x24, 0xAA);
pub const RESET: (u8, u8, u8) = (0x79, 0x55, 0x48);
pub const BUS: (u8, u8, u8) = (0x15, 0x65, 0xC0);

/// Wire colour and width (px) for a net class; `dark` picks the brighter set for a black canvas.
pub fn class_style(c: crate::net::NetClass, dark: bool) -> ((u8, u8, u8), f32) {
    use crate::net::NetClass::*;
    let (rgb, w) = match c {
        Clock => (CLOCK, 1.5),
        Reset => (RESET, 1.5),
        Bus => (BUS, 2.0),
        Signal => (SIGNAL, 1.5),
        Undeclared => (UNDECLARED, 1.5),
    };
    let bright = match c {
        Clock => (0xCE, 0x93, 0xD8),
        Reset => (0xD7, 0xB0, 0x8A),
        Bus => (0x64, 0xB5, 0xF6),
        Signal => (0x66, 0xBB, 0x6A),
        Undeclared => (0x90, 0xA4, 0xAE),
    };
    (if dark { bright } else { rgb }, w)
}
