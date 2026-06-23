pub trait Mapper {
    fn cpu_read(&self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);
    fn ppu_read(&self, addr: u16) -> u8;
    fn ppu_write(&mut self, addr: u16, value: u8);
    fn mirror_vram_address(&self, addr: u16) -> usize;
    fn is_irq_asserted(&self) -> bool { false }
    fn update_cycles(&mut self, _cycles: u64) {}
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mirroring {
	Horizontal,
	Vertical,
	SingleLower, // Maps everything to $2000 (VRAM 0-1023)
	SingleUpper, // Maps everything to $2400 (VRAM 1024-2047)
	FourScreen,
}

// Mapper 0 (NROM) - Standard flat cartridge, no bank switching
pub struct Mapper0 {
    prg_banks: usize,
    mirroring_mode: Mirroring,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
}

impl Mapper0 {
    pub fn new(prg_banks: usize, chr_banks: usize, prg_rom: Vec<u8>, chr_rom: Vec<u8>, initial_mirroring: Mirroring) -> Self {
        Self {
            prg_banks,
            mirroring_mode: initial_mirroring,
            prg_rom,
            chr_rom,
        }
    }
}

impl Mapper for Mapper0 {
    fn cpu_read(&self, addr: u16) -> u8 {
        let mut rom_addr = addr - 0x8000;
        if self.prg_rom.len() == 16384 {
            rom_addr %= 16384; // Mirroring for 16KB games
        }
        self.prg_rom[rom_addr as usize]
    }
    fn cpu_write(&mut self, _addr: u16, _value: u8) {
        // NROM is read-only!
    }
    fn ppu_read(&self, addr: u16) -> u8 {
        self.chr_rom[addr as usize]
    }
    fn ppu_write(&mut self, addr: u16, value: u8) {
        // handle chr_ram writes or modifications if needed
    }
    fn mirror_vram_address(&self, addr: u16) -> usize {
        let normalized = (addr & 0x0FFF) as usize; // Map $2000-$2FFF to $000-$FFF
        match self.mirroring_mode {
            Mirroring::Horizontal => {
                // Nametables 0 and 1 map to first 1KB; Nametables 2 and 3 map to second 1KB
                if normalized < 0x800 {
                    normalized % 0x400
                } else {
                    0x400 + (normalized % 0x400)
                }
            }
            Mirroring::Vertical => {
                // Nametables 0 and 2 map to first 1KB; Nametables 1 and 3 map to second 1KB
                normalized % 0x800
            }
            _ => normalized % 2048,
        }
    }
}