pub struct CpuConfig {
    has_bcd: bool,
    has_jmp_bug: bool,
    is_c02: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CpuVariant {
    ZilogZ80,
    ZilogZ180,
}

pub struct Z80Registers {
    pub a: u8, pub f: u8,
    pub b: u8, pub c: u8,
    pub d: u8, pub e: u8,
    pub h: u8, pub l: u8,

    // Shadow set, swapped in via EX AF,AF' / EXX
    pub a_: u8, pub f_: u8,
    pub b_: u8, pub c_: u8,
    pub d_: u8, pub e_: u8,
    pub h_: u8, pub l_: u8,

    pub ix: u16,
    pub iy: u16,
    pub sp: u16,
    pub pc: u16,
    pub i: u8,   // interrupt vector
    pub r: u8,   // memory refresh
}

impl Z80Registers {
    pub fn bc(&self) -> u16 { ((self.b as u16) << 8) | self.c as u16 }
    pub fn set_bc(&mut self, v: u16) { self.b = (v >> 8) as u8; self.c = v as u8; }

    pub fn de(&self) -> u16 { ((self.d as u16) << 8) | self.e as u16 }
    pub fn set_de(&mut self, v: u16) { self.d = (v >> 8) as u8; self.e = v as u8; }

    pub fn hl(&self) -> u16 { ((self.h as u16) << 8) | self.l as u16 }
    pub fn set_hl(&mut self, v: u16) { self.h = (v >> 8) as u8; self.l = v as u8; }

    pub fn af(&self) -> u16 { ((self.a as u16) << 8) | self.f as u16 }
    pub fn set_af(&mut self, v: u16) { self.a = (v >> 8) as u8; self.f = v as u8; }

    pub fn read_r(&self, idx: u8, bus: &mut dyn Bus) -> u8 {
        match idx & 0x07 {
            0 => self.b, 1 => self.c, 2 => self.d, 3 => self.e,
            4 => self.h, 5 => self.l,
            6 => bus.read_byte(self.hl()),   // (HL) — the one index that hits memory, not a register
            7 => self.a,
            _ => unreachable!(),
        }
    }

    pub fn write_r(&mut self, idx: u8, value: u8, bus: &mut dyn Bus) {
        match idx & 0x07 {
            0 => self.b = value, 1 => self.c = value, 2 => self.d = value, 3 => self.e = value,
            4 => self.h = value, 5 => self.l = value,
            6 => bus.write_byte(self.hl(), value),
            7 => self.a = value,
            _ => unreachable!(),
        }
    }
}

pub struct Z80Cpu {
    pc: u16,
    sp: usize,
    regs: Z80Registers,
    nmi_pending: bool,
    prev_nmi_line: bool,
    last_cycles: u8,
    last_opcode: u8, // Save most recent instruction for debugging
    operand_address_crossed_page: bool,
    pub total_cycles: u64,
    pub is_running: bool,
    pub config: CpuConfig,
}

impl Z80Cpu {
    pub fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
        }
    }
}