pub const GRID_OPTIONS: [(&str, i32); 2] = [("40 (coarse)", 40), ("20 (medium)", 20)];
pub const DEFAULT_GRID: i32 = 20;
pub const MIN_BLOCK_WIDTH: i32 = 150;
pub const MIN_BLOCK_HEIGHT: i32 = 90;
pub const GRID_STEP: i32 = 10;
pub const PORT_HEIGHT: i32 = 20;

// Colour table from config.py, as (r, g, b).
pub const SIGNAL: (u8, u8, u8) = (0x4C, 0xAF, 0x50);
pub const VARIABLE: (u8, u8, u8) = (0x9C, 0x27, 0xB0);
pub const CONSTANT: (u8, u8, u8) = (0xFF, 0x98, 0x00);
pub const UNDECLARED: (u8, u8, u8) = (0x60, 0x7D, 0x8B);
pub const HIGHLIGHT: (u8, u8, u8) = (0xFF, 0x6F, 0x00);
pub const INPUT_PORT: (u8, u8, u8) = (0x21, 0x96, 0xF3);
pub const OUTPUT_PORT: (u8, u8, u8) = (0xF4, 0x43, 0x36);
pub const INSTANCE_BG: (u8, u8, u8) = (0xE8, 0xF4, 0xF8);
