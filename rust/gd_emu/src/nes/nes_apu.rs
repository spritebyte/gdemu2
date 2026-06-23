pub struct NesAPU {
    frame_counter: u32,
    mode_5_step: bool,
    irq_inhibit: bool,
    frame_irq_flag: bool,
}

impl NesAPU {
    pub fn new() -> Self {
        Self { frame_counter: 0, mode_5_step: false, irq_inhibit: false, frame_irq_flag: false }
    }

    pub fn write_4017(&mut self, value: u8) {
        self.mode_5_step = (value & 0x80) != 0;
        self.irq_inhibit = (value & 0x40) != 0;
        if self.irq_inhibit { self.frame_irq_flag = false; }
        self.frame_counter = 0; // real hardware also resets the sequencer here
    }

    pub fn read_4015(&mut self) -> u8 {
        let result = if self.frame_irq_flag { 0x40 } else { 0 };
        self.frame_irq_flag = false; // reading $4015 clears the IRQ flag
        result
    }

    pub fn is_irq_asserted(&self) -> bool {
        self.frame_irq_flag && !self.irq_inhibit
    }

    /// Step by CPU cycles (not PPU cycles — no *3 here)
    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.frame_counter += 1;
            if !self.mode_5_step && self.frame_counter >= 29830 {
                if !self.irq_inhibit { self.frame_irq_flag = true; }
                self.frame_counter = 0;
            } else if self.mode_5_step && self.frame_counter >= 37282 {
                self.frame_counter = 0; // 5-step mode never asserts IRQ
            }
        }
    }
}