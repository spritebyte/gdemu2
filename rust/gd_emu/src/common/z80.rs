pub struct M6502Cpu {
    pc: u16,
    sp: usize,
    a: u8,
    f: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
}

impl M6502Cpu {
    pub fn new() -> Self {
        Self {
            pc: 0,
            sp: 0,
        }
    }
}