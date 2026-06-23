use crate::nes::mappers::{Mapper,Mirroring};

// Mapper 2 (UxROM)
pub struct Mapper2 {
    prg_banks: usize,
    prg_bank: u8,
    chr_banks: usize,
    mirroring_mode: Mirroring,
    has_four_screen: bool,

    prg_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_rom: Vec<u8>,
    chr_ram: Vec<u8>,
    current_cycle: i64,
    sram_dirty: bool,
}

impl Mapper2 {
    pub fn new(prg_banks: usize, chr_banks: usize, prg_rom: Vec<u8>, chr_rom: Vec<u8>, initial_mirroring: Mirroring, four_screen_bit: bool) -> Self {
        let prg_ram = vec![0; 8192];
        let chr_ram = if chr_banks == 0 { vec![0; 8192] } else { vec![] };

        Self {
            prg_banks,
            prg_bank: 0,
            chr_banks,
            mirroring_mode: initial_mirroring,
            has_four_screen: four_screen_bit,
            prg_rom,
            prg_ram,
            chr_rom,
            chr_ram,
            current_cycle: 0,
            sram_dirty: false,
        }
    }
}

impl Mapper for Mapper2 {
    fn update_cycles(&mut self, cycles: u64) {
        self.current_cycle += cycles as i64;
    }

    fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 && addr <= 0xBFFF {
            // Switchable bank
            let bank = self.prg_bank as usize % self.prg_banks;
            let offset = (addr - 0x8000) as usize;
            let target = (bank * 0x4000) + offset;
            return self.prg_rom[target];
        }
        else if addr >= 0xC000 && addr <= 0xFFFF {
            // FIXED bank (always the last bank)
            let bank = self.prg_banks - 1;
            let offset = (addr - 0xC000) as usize;
            let target = (bank * 0x4000) + offset;
            return self.prg_rom[target]
        }
        0
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        if addr >= 0x8000 {
            self.prg_bank = value % (self.prg_banks as u8);
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        if addr < 0x2000 {
            if self.chr_banks == 0 {
                return self.chr_ram[addr as usize];
            } else {
                return self.chr_rom[addr as usize];
            }
        }
        0
    }

    fn ppu_write(&mut self, p_addr: u16, value: u8) {
        let addr = p_addr & 0x3FFF;

        if addr < 0x2000 {
            if self.chr_banks == 0 {
                self.chr_ram[addr as usize] = value;
            }
        }
    }


    fn mirror_vram_address(&self, addr: u16) -> usize {
        let normalized = (addr & 0x0FFF) as usize; // Map $2000-$2FFF to $000-$FFF
        if self.has_four_screen {
            return normalized;
        }

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
            Mirroring::SingleLower => normalized % 0x400,
            Mirroring::SingleUpper => 0x400 + (normalized % 0x400),
            _ => normalized,
        }
    }
}