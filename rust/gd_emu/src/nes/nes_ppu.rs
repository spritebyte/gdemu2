use godot::prelude::*;
use godot::classes::ImageTexture;
use crate::nes::mappers::Mapper;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// The standard 64-color original NES system hardware color palette
const NES_PALETTE: [(u8, u8, u8); 64] = [
    (84, 84, 84),    (0, 30, 116),    (8, 16, 144),    (48, 0, 136),    (68, 0, 100),    (92, 0, 48),     (84, 4, 0),      (60, 24, 0),
    (32, 42, 0),     (8, 58, 0),      (0, 64, 0),      (0, 60, 0),      (0, 50, 60),     (0, 0, 0),       (0, 0, 0),       (0, 0, 0),
    (152, 150, 152), (8, 76, 196),    (48, 50, 236),   (92, 30, 228),   (136, 20, 176),  (160, 20, 116),  (152, 34, 32),   (112, 64, 0),
    (72, 88, 0),     (24, 114, 0),    (0, 124, 0),     (0, 118, 40),    (0, 102, 120),   (0, 0, 0),       (0, 0, 0),       (0, 0, 0),
    (236, 238, 236), (76, 154, 236),  (120, 124, 236), (176, 98, 236),  (228, 84, 236),  (236, 88, 180),  (236, 106, 100), (212, 136, 32),
    (160, 170, 0),   (116, 196, 0),   (76, 208, 32),   (56, 204, 108),  (56, 180, 204),  (60, 60, 60),    (0, 0, 0),       (0, 0, 0),
    (236, 238, 236), (168, 204, 236), (188, 194, 236), (212, 178, 236), (236, 174, 236), (236, 174, 212), (236, 180, 176), (228, 196, 144),
    (204, 210, 120), (180, 222, 120), (168, 226, 144), (152, 226, 180), (160, 214, 228), (160, 162, 160), (0, 0, 0),       (0, 0, 0),
];

struct SpritePixelInfo {
    color_bit: u8,
    palette_idx: u8,
    priority: u8,
    is_sprite_0: bool,
}

pub struct NesPPU {
    // --- Hardware Registers ---
    ctrl: u8,       // $2000
    mask: u8,       // $2001
    status: u8,     // $2002
    oam_addr: u8,   // $2003
    scroll: u16,    // $2005 internal latches
    addr: u16,      // $2006 internal latches

    // Addresses and internal latches
    base_nametable_address: u16,
    vram_increment:u8,
    sprite_pattern_table:u16,
    background_pattern_table: u16,
    sprite_size:u8,
    fine_x:u8,
    w_latch:u8,
    pub v_addr: u16,  // Current VRAM read/write pointer address (15 bits)
    pub t_addr: u16,  // Temporary internal address latch

    // memory blocks
    pub vram: [u8; 2048],
    pub palette_ram: [u8; 32],
    pub oam: [u8; 256], // 64 sprites * 4 bytes each
    data_buffer: u8,    // Delayed reading cache buffer for PPUDATA ($2007)

    // --- Timing & Synchronization ---
    pub frame_ready: bool,
    scanline: i16,   // -1 to 261
    cycle: i16,      // 0 to 340
    
    // --- Video Buffers ---
    // Double buffering prevents the UI thread from reading half-rendered frames!
    back_buffer: Vec<u8>,       // The frame currently being drawn (Width * Height * 4 bytes RGBA)
    front_buffer: Arc<Vec<u8>>, // The last fully completed frame, safe for sharing across threads
    
    // Direct shared reference to the system's atomic sync flag
    system_frame_ready: Arc<AtomicBool>,
}

impl NesPPU {
    pub fn new(system_frame_ready: Arc<AtomicBool>) -> Self {
        let buffer_size = 256 * 240 * 4; // NES Resolution: 256x240, 4 bytes per pixel (RGBA8)
        Self {
            ctrl: 0, mask: 0, status: 0, oam_addr: 0, scroll: 0, addr: 0,
            base_nametable_address: 0x2000, vram_increment: 1,
            sprite_pattern_table: 0x0, background_pattern_table: 0x0, sprite_size: 8,
            frame_ready: false, scanline: 0, cycle: 0, w_latch: 0, fine_x: 0,
            v_addr: 0, t_addr: 0, data_buffer: 0,
            vram: [0;2048], palette_ram: [0;32], oam: [0;256],
            back_buffer: vec![0; buffer_size],
            front_buffer: Arc::new(vec![0; buffer_size]),
            system_frame_ready,
        }
    }

    pub fn init(&mut self) {

    }

    pub fn reset(&mut self) {
        self.ctrl = 0; self.mask = 0; self.status = 0;
        self.scanline = 0; self.cycle = 0; self.frame_ready = false;
        self.w_latch = 0; self.v_addr = 0; self.t_addr = 0;
    }

    /// Step the PPU by a designated number of clock cycles (typically CPU cycles * 3)
    pub fn step(&mut self, mapper: &dyn Mapper, cycles: u32) {
        for _ in 0..cycles {
            if self.scanline >= 0 && self.scanline < 240 {
                if self.cycle >= 1 && self.cycle <= 256 {
                    self.render_pixel(mapper);
                }
            }

            if self.mask & 0x18 != 0 {
                if (self.scanline >= 0 && self.scanline < 240) || self.scanline == -1 {
                    if self.cycle == 256 {
                        self.increment_vertical_scroll();
                    }
                    else if self.cycle == 257 {
                        self.v_addr = (self.v_addr & ! 0x041F) | (self.t_addr & 0x041F);
                    }
                }
                if self.scanline == -1 && self.cycle == 304 {
                    self.v_addr = (self.v_addr & !0x7BE0) | (self.t_addr & 0x7BE0);
                }
            }

            self.cycle += 1;
            if self.cycle > 340 {
                self.cycle = 0;
                self.scanline += 1;

                // enter v-blank at scanline 241. 
                if self.scanline == 241 {
                    if (self.status & 0x80) == 0 {
                        self.status |= 0x80;
                    }
                }

                if self.scanline > 261 {
                    self.scanline = -1;     // Wrap back around to Pre-render scanline
                    self.status &= 0x3F;    // reset v-blank and sprite-0 hit
                    // --- V-Blank Finished / Frame Completed ---
                    self.frame_ready = true;
                    
                    // Swap the buffers: Move completed image to front, acquire fresh back buffer
                    // This atomic operation takes less than a microsecond!
                    let mut completed_buffer = std::mem::replace(&mut self.back_buffer, vec![0; 256 * 240 * 4]);
                    
                    // Protect against thread collision by atomically updating the front pointer
                    self.front_buffer = Arc::new(completed_buffer);
                    
                    // Signal the UI thread that a brand new frame is sitting in memory waiting to be drawn
                    self.system_frame_ready.store(true, Ordering::Release);
                }
            }
        }
    }

    fn increment_vertical_scroll(&mut self) {
        if self.v_addr & 0x7000 != 0x7000 {  // if fine Y < 7
            self.v_addr += 0x1000;            // increment fine Y
        }
        else {
            self.v_addr &= !0x7000;    // Fine Y = 0
            let mut y = (self.v_addr & 0x03E0) >> 5; // Coarse Y
            if y == 29 {                // if at bottom of name table
                y = 0;
                self.v_addr ^= 0x0800;       // switch vertical nametable
            }
            else if y == 31 {
                y = 0
            }
            else {
                y += 1;
            }
            self.v_addr = (self.v_addr & !0x03E0) | (y << 5);
        }
    }

    fn render_pixel(&mut self, mapper: &dyn Mapper) {
        let x = (self.cycle - 1) as usize;
        let y = self.scanline as usize;
        let pixel_index = (y * 256 + x) * 4;

        // Fallback color if background layer rendering is disabled in PPUMASK
        if (self.mask & 0x08) == 0 {
            let color_idx = self.ppu_read(mapper, 0x3F00) & 0x3F;
            let (r, g, b) = NES_PALETTE[color_idx as usize];
            self.back_buffer[pixel_index] = r;
            self.back_buffer[pixel_index + 1] = g;
            self.back_buffer[pixel_index + 2] = b;
            self.back_buffer[pixel_index + 3] = 255;
            return;
        }

        // --- SCROLLING BACKGROUND MATHEMATICS (Loopy Architecture) ---
        // Extract base scroll coordinates directly from v_addr
        let start_coarse_x = (self.v_addr & 0x001F) as usize;
        let coarse_y = ((self.v_addr >> 5) & 0x001F) as usize;
        let start_nt_h = ((self.v_addr >> 10) & 0x01) as usize;
        let nt_v = ((self.v_addr >> 11) & 0x01) as usize;
        let fine_y = ((self.v_addr >> 12) & 0x0007) as usize;

        // Calculate absolute horizontal coordinates across the current nametable boundaries
        let total_x = (start_coarse_x * 8) + (start_nt_h * 256) + (self.fine_x as usize) + x;
        let tile_x = (total_x / 8) % 32;
        let nt_h = (total_x / 256) % 2;
        let fine_x = total_x % 8;

        // Fetch Tile ID from Nametable layout memory
        let base_nt = 0x2000 + ((nt_v << 1) | nt_h) as u16 * 0x400;
        let nt_addr = base_nt + (coarse_y * 32 + tile_x) as u16;
        let tile_id = self.ppu_read(mapper, nt_addr);

        // Fetch raw image lines from CHR-ROM Pattern Table
        let pattern_addr = self.background_pattern_table + (tile_id as u16 * 16) + fine_y as u16;
        let low_byte = self.ppu_read(mapper, pattern_addr);
        let high_byte = self.ppu_read(mapper, pattern_addr + 8);

        // Split out the 2-bit local tile color index
        let bit_shift = 7 - fine_x;
        let pixel_color_bit = ((low_byte >> bit_shift) & 0x01) | (((high_byte >> bit_shift) & 0x01) << 1);

        // Fetch Palette quadrant groupings from Attribute Table
        let attr_table_addr = base_nt + 0x03C0 + ((coarse_y / 4) * 8 + (tile_x / 4)) as u16;
        let attr_byte = self.ppu_read(mapper, attr_table_addr);
        let quadrant_x = (tile_x % 4) / 2;
        let quadrant_y = (coarse_y % 4) / 2;
        let attr_shift = (quadrant_y * 2 + quadrant_x) * 2;
        let palette_idx = (attr_byte >> attr_shift) & 0x03;

        // Match color profiles
        let palette_base = 0x3F00 + (palette_idx as u16 * 4);
        let final_color_addr = if pixel_color_bit == 0 {
            0x3F00 
        } else {
            palette_base + pixel_color_bit as u16
        };

        let color_idx = self.ppu_read(mapper, final_color_addr) & 0x3F;
        let (r, g, b) = NES_PALETTE[color_idx as usize];

        // Store the final color values to write
        let mut final_r = r;
        let mut final_g = g;
        let mut final_b = b;

        // --- SPRITE LAYER OVERLAY ---
        if (self.mask & 0x10) != 0 { // Check if sprites are enabled in PPUMASK
            let height = self.sprite_size as usize;
            
            // Loop backwards from 63 down to 0. 
            // This ensures lower OAM indices (like Sprite 0) are drawn LAST, 
            // correctly overwriting higher index sprites when they overlap.
            for i in (0..64).rev() {
                let oam_idx = i * 4;
                let sprite_y = self.oam[oam_idx] as usize;
                let sprite_tile = self.oam[oam_idx + 1];
                let sprite_attr = self.oam[oam_idx + 2];
                let sprite_x = self.oam[oam_idx + 3] as usize;

                // NES hardware delays sprite evaluation by exactly 1 scanline line offset
                if y >= sprite_y + 1 && y < sprite_y + 1 + height {
                    let mut fine_y = y - (sprite_y + 1);
                    if (sprite_attr & 0x80) != 0 { // Attribute Bit 7: Vertical Flip
                        fine_y = height - 1 - fine_y;
                    }

                    if x >= sprite_x && x < sprite_x + 8 {
                        let mut fine_x = x - sprite_x;
                        if (sprite_attr & 0x40) != 0 { // Attribute Bit 6: Horizontal Flip
                            fine_x = 7 - fine_x;
                        }

                        // Resolve CHR Pattern Table addresses for 8x8 vs 8x16 configurations
                        let table = if height == 16 {
                            ((sprite_tile & 0x01) as u16) * 0x1000
                        } else {
                            self.sprite_pattern_table
                        };

                        let actual_tile = if height == 16 {
                            sprite_tile & 0xFE
                        } else {
                            sprite_tile
                        };

                        let mut final_fine_y = fine_y;
                        let mut tile_offset = 0u16;
                        if height == 16 && fine_y >= 8 {
                            tile_offset = 1;
                            final_fine_y -= 8;
                        }

                        let pattern_addr = table + ((actual_tile as u16 + tile_offset) * 16) + final_fine_y as u16;
                        let low_byte = self.ppu_read(mapper, pattern_addr);
                        let high_byte = self.ppu_read(mapper, pattern_addr + 8);

                        let bit_shift = 7 - fine_x;
                        let sprite_pixel_bit = ((low_byte >> bit_shift) & 0x01) | (((high_byte >> bit_shift) & 0x01) << 1);

                        // 0 indicates a transparent sprite pixel—ignore and let background show through
                        if sprite_pixel_bit != 0 { 
                            if i == 0 && pixel_color_bit != 0 && x < 255 {
                                if (self.mask & 0x08) != 0 {
                                    self.status |= 0x40;
                                }
                            }
                            // Sprite Palettes live at memory space index ranges 4-7 ($3F10-$3FFF)
                            let sprite_palette_idx = (sprite_attr & 0x03) + 4;
                            let palette_base = 0x3F00 + (sprite_palette_idx as u16 * 4);
                            let sprite_color_addr = palette_base + sprite_pixel_bit as u16;

                            let s_color_idx = self.ppu_read(mapper, sprite_color_addr) & 0x3F;
                            let (sr, sg, sb) = NES_PALETTE[s_color_idx as usize];

                            // Attribute Bit 5: Priority (0 = In Front, 1 = Behind Background)
                            let bg_transparent = pixel_color_bit == 0;
                            if (sprite_attr & 0x20) == 0 || bg_transparent {
                                final_r = sr;
                                final_g = sg;
                                final_b = sb;
                            }
                        }
                    }
                }
            }
        }

        // Commit final composition color data to back-buffer pixel arrays
        self.back_buffer[pixel_index] = final_r;
        self.back_buffer[pixel_index + 1] = final_g;
        self.back_buffer[pixel_index + 2] = final_b;
        self.back_buffer[pixel_index + 3] = 255;
    }

    fn get_sprite_pixel(&self, mapper: &dyn Mapper, x: usize, y: usize) -> Option<SpritePixelInfo> {
        let sprite_height = self.sprite_size as usize; // 8 or 16

        // Scan through all 64 sprites in OAM
        // OAM index 0 has highest priority, so the first opaque sprite pixel we hit wins!
        for i in 0..64 {
            let oam_base = i * 4;
            let sprite_y = self.oam[oam_base] as usize;
            let tile_id = self.oam[oam_base + 1];
            let attributes = self.oam[oam_base + 2];
            let sprite_x = self.oam[oam_base + 3] as usize;

            // NES sprites are delayed by 1 scanline in hardware. 
            // A sprite with Y=0 in OAM actually starts rendering on scanline 1.
            let actual_y = sprite_y + 1;

            // Check if the current pixel coordinate falls inside this sprite bounding box
            if y >= actual_y && y < actual_y + sprite_height {
                if x >= sprite_x && x < sprite_x + 8 {
                    
                    // Determine internal offsets inside the 8x8 or 8x16 sprite tile
                    let mut fine_y = y - actual_y;
                    let mut fine_x = x - sprite_x;

                    // Parse Attribute Byte flags
                    let flip_horizontal = (attributes & 0x40) != 0;
                    let flip_vertical = (attributes & 0x80) != 0;
                    let priority = (attributes & 0x20) >> 5; // 0 = Front, 1 = Behind
                    let palette_idx = attributes & 0x03;

                    // Handle Flipping
                    if flip_horizontal { fine_x = 7 - fine_x; }
                    if flip_vertical { fine_y = (sprite_height - 1) - fine_y; }

                    // Fetch the correct Pattern Table address for the sprite pixel
                    let pattern_addr = if sprite_height == 8 {
                        // 8x8 Sprite Mode
                        self.sprite_pattern_table + (tile_id as u16 * 16) + fine_y as u16
                    } else {
                        // 8x16 Sprite Mode (Used by many advanced games, though Mario uses 8x8)
                        // Bit 0 of tile_id determines the pattern table bank ($0000 or $1000)
                        let bank = (tile_id & 0x01) as u16 * 0x1000;
                        let mut actual_tile = tile_id & 0xFE;
                        if fine_y >= 8 {
                            actual_tile += 1;
                            fine_y -= 8;
                        }
                        bank + (actual_tile as u16 * 16) + fine_y as u16
                    };

                    let low_byte = self.ppu_read(mapper, pattern_addr);
                    let high_byte = self.ppu_read(mapper, pattern_addr + 8);

                    let bit_shift = 7 - fine_x;
                    let color_bit = ((low_byte >> bit_shift) & 0x01) | (((high_byte >> bit_shift) & 0x01) << 1);

                    // If this pixel is not transparent, we found our sprite color!
                    if color_bit != 0 {
                        return Some(SpritePixelInfo {
                            color_bit,
                            palette_idx,
                            priority,
                            is_sprite_0: i == 0, // Sprite 0 is the very first entry in OAM
                        });
                    }
                }
            }
        }
        None
    }

    /// Exposes a thread-safe read clone of the completed pixel array
    pub fn get_front_buffer(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.front_buffer)
    }

    pub fn is_in_vblank(&self) -> bool {
        return (self.status & 0x80) == 0x80;
    }

    pub fn is_nmi_enabled(&self) -> bool {
        return (self.ctrl & 0x80) == 0x80;
    }

    pub fn ppu_read(&self, mapper: &dyn Mapper, mut addr: u16) -> u8 {
        addr &= 0x3FFF;

        match addr {
            0x0000..=0x1FFF => mapper.ppu_read(addr),
            0x2000..=0x3EFF => self.vram[mapper.mirror_vram_address(addr) % 2048],
            0x3F00..=0x3FFF => {
                let mut palette_addr = (addr & 0x001F) as usize;
                if palette_addr >= 0x10 && (palette_addr % 4 == 0) { palette_addr -= 0x10; }
                self.palette_ram[palette_addr]
            }
            _ => 0,
        }
    }

    pub fn ppu_write(&mut self, mapper: &mut dyn crate::nes::mappers::Mapper, mut addr: u16, value: u8) {
        addr &= 0x3FFF;

        match addr {
            0x0000..=0x1FFF => {
                mapper.ppu_write(addr, value);
            }
            0x2000..=0x3EFF => {
                let mirrored_addr = mapper.mirror_vram_address(addr);
                self.vram[mirrored_addr % 2048] = value;
            }
            0x3F00..=0x3FFF => {
                let mut palette_addr = (addr & 0x001F) as usize;
                if palette_addr >= 0x10 && (palette_addr % 4 == 0) {
                    palette_addr -= 0x10;
                }
                self.palette_ram[palette_addr] = value;
            }
            _ => {}
        }
    }

    pub fn cpu_read_reg(&mut self, mapper: &dyn Mapper, reg: u16) -> u8 {
        match reg {
            2 => { // $2002 - PPUSTATUS
                let res = self.status;
                self.status &= 0x7F; // Reading status clears V-Blank bit
                self.w_latch = 0;    // And resets scroll/address double-write latch
                res
            }
            7 => { // $2007 - PPUDATA
                let mut data = self.ppu_read(mapper, self.v_addr);
                if self.v_addr < 0x3F00 {
                    let buffered_data = self.data_buffer;
                    self.data_buffer = data;
                    data = buffered_data;
                } else {
                    self.data_buffer = self.ppu_read(mapper, self.v_addr - 0x1000);
                }
                self.v_addr = self.v_addr.wrapping_add(self.vram_increment as u16);
                data
            }
            _ => 0
        }
    }

    pub fn cpu_write_reg(&mut self, mapper: &mut dyn crate::nes::mappers::Mapper, reg: u16, value: u8) {
        match reg {
            0 => { // $2000 - PPUCTRL
                let old_ctrl = self.ctrl;
                self.ctrl = value;
                // Extract bits to configure scrolling targets
                self.t_addr = (self.t_addr & 0xF3FF) | (((value & 0x03) as u16) << 10);
                self.vram_increment = if (value & 0x04) == 0x04 { 32 } else { 1 };
                self.background_pattern_table = if (value & 0x10) == 0x10 { 0x1000 } else { 0x0000 };
                self.sprite_pattern_table = if (value & 0x08) == 0x08 { 0x1000 } else { 0x0000 };
                self.sprite_size = if (value & 0x20) == 0x20 { 16 } else { 8 };
            }
            1 => { // $2001 - PPUMASK
                self.mask = value;
            }
            3 => { // $2003 - OAMADDR
                self.oam_addr = value;
            }
            5 => { // $2005 - PPUSCROLL
                if self.w_latch == 0 {
                    // First write: Coarse X and Fine X scrolling values
                    self.t_addr = (self.t_addr & 0x7FE0) | ((value >> 3) as u16);
                    self.fine_x = value & 0x07;
                    self.w_latch = 1;
                } else {
                    // Second write: Coarse Y and Fine Y scrolling values
                    self.t_addr = (self.t_addr & 0x0C1F) | (((value & 0x07) as u16) << 12) | (((value >> 3) as u16) << 5);
                    self.w_latch = 0;
                }
            }
            6 => { // $2006 - PPUADDR
                if self.w_latch == 0 {
                    // First write: High byte of the 14-bit destination target address
                    self.t_addr = (self.t_addr & 0x00FF) | (((value & 0x3F) as u16) << 8);
                    self.w_latch = 1;
                } else {
                    // Second write: Low byte of destination target address
                    self.t_addr = (self.t_addr & 0xFF00) | (value as u16);
                    self.v_addr = self.t_addr; // Latch copies address into current VRAM target
                    self.w_latch = 0;
                }
            }
            7 => { // $2007 - PPUDATA
                // Write the value into the destination VRAM address
                self.ppu_write(mapper, self.v_addr, value);
                // Automatically step forward the target address based on $2000 setup configurations
                self.v_addr = self.v_addr.wrapping_add(self.vram_increment as u16);
            }
            _ => {}
        }
    }

    pub fn write_oam_dma(&mut self, data: &[u8; 256]) {
        self.oam.copy_from_slice(data);
    }

    pub fn is_nmi_line_asserted(&self) -> bool {
        let nmi_occurred = (self.status & 0x80) != 0;
        let nmi_output = (self.ctrl & 0x80) != 0;
        nmi_occurred && nmi_output
    }
}