const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20,  2, 40,  4, 80,  6, 160,  8, 60, 10, 14, 12, 26, 14,
    12,  16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
];

const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
    [0, 1, 1, 0, 0, 0, 0, 0], // 25%
    [0, 1, 1, 1, 1, 0, 0, 0], // 50%
    [1, 0, 0, 1, 1, 1, 1, 1], // 25% inverted
];

const TRI_SEQUENCE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
];

const NOISE_PERIOD_TABLE: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2032, 4064
];

pub struct NesAPU {
    frame_counter: u32,
    mode_5_step: bool,
    irq_inhibit: bool,
    frame_irq_flag: bool,
    
    // Pulse 1 Registers & Components
    p1_timer_reload: u16,
    p1_timer: u16,
    p1_sequence_step: u8,
    p1_length_counter: u8,
    p1_duty_index: u8,
    p1_volume: u8,
    pulse1_enabled: bool,
    p1_halt: bool,

    // Pulse 2 Registers & Components
    p2_timer_reload: u16,
    p2_timer: u16,
    p2_sequence_step: u8,
    p2_length_counter: u8,
    p2_duty_index: u8,
    p2_volume: u8,
    pulse2_enabled: bool,
    p2_halt: bool,

    // Triangle Channel State
    tri_enabled: bool,
    tri_reload_flag: bool,
    tri_timer_reload: u16,
    tri_timer: u16,
    tri_sequence_step: u8,
    tri_length_counter: u8,
    tri_linear_counter: u8,
    tri_linear_reload: u8,
    tri_control_flag: bool,
    
    // Noise Channel Registers
    n_halt: bool,
    n_constant_volume: bool,
    n_volume: u8,
    n_mode: bool,
    n_timer_reload: u16,
    n_timer: u16,
    n_shift_register: u16, // Fixed to u16 for 15-bit LFSR
    noise_enabled: bool,
    noise_length_counter: u8,

    // Envelopes
    p1_env_volume: u8,
    p1_env_divider: u8,
    p2_env_volume: u8,
    p2_env_divider: u8,
    n_env_volume: u8,
    n_env_divider: u8,

    // Audio Tracking
    audio_buffer: Vec<f32>,
    sample_clock: f32,
    sample_rate_ratio: f32,
    p1_output: f32,
    p2_output: f32,
}

impl NesAPU {
    pub fn new() -> Self {
        Self { 
            frame_counter: 0, 
            mode_5_step: false, 
            irq_inhibit: false, 
            frame_irq_flag: false,
            audio_buffer: Vec::with_capacity(1000),
            sample_clock: 0.0,
            sample_rate_ratio: 1789773.0 / 44100.0,
            p1_timer_reload: 0,
            p1_timer: 0,
            p1_sequence_step: 0,
            p1_length_counter: 0,
            p1_duty_index: 0,
            p1_volume: 0,
            pulse1_enabled: false,
            p1_halt: false,
            p1_output: 0.0,
            p2_timer_reload: 0,
            p2_timer: 0,
            p2_sequence_step: 0,
            p2_length_counter: 0,
            p2_duty_index: 0,
            p2_volume: 0,
            pulse2_enabled: false,
            p2_halt: false,
            p2_output: 0.0,
            tri_enabled: false,
            tri_reload_flag: false,
            tri_timer_reload: 0,
            tri_timer: 0,
            tri_sequence_step: 0,
            tri_length_counter: 0,
            tri_linear_counter: 0,
            tri_linear_reload: 0,
            tri_control_flag: false,
            n_halt: false,
            n_constant_volume: false,
            n_volume: 0,
            n_mode: false,
            n_timer_reload: 0,
            n_timer: 0,
            n_shift_register: 1, // Must be initialized to 1!
            noise_enabled: false,
            noise_length_counter: 0,
            p1_env_volume: 0,
            p1_env_divider: 0,
            p2_env_volume: 0,
            p2_env_divider: 0,
            n_env_volume: 0,
            n_env_divider: 0,
        }
    }

    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.audio_buffer)
    }

    pub fn write_reg(&mut self, addr: u16, data: u8) {
        match addr {
            // Pulse 1
            0x4000 => { 
                self.p1_duty_index = (data >> 6) & 0x03; 
                self.p1_halt = (data & 0x20) > 0;
                self.p1_volume = data & 0x0F;
            }
            0x4002 => { 
                self.p1_timer_reload = (self.p1_timer_reload & 0x0700) | (data as u16);
            }
            0x4003 => { 
                self.p1_timer_reload = (self.p1_timer_reload & 0x00FF) | (((data & 0x07) as u16) << 8); 
                self.p1_sequence_step = 0; 
                self.write_p1_length(data);
            }
            
            // Pulse 2
            0x4004 => {
                self.p2_duty_index = (data >> 6) & 0x03;
                self.p2_halt = (data & 0x20) > 0;
                self.p2_volume = data & 0x0F;
            }
            0x4006 => {
                self.p2_timer_reload = (self.p2_timer_reload & 0x0700) | (data as u16);
            }
            0x4007 => {
                self.p2_timer_reload = (self.p2_timer_reload & 0x00FF) | (((data & 0x07) as u16) << 8);
                self.p2_sequence_step = 0;
                self.write_p2_length(data);
            }

            // Triangle
            0x4008..=0x400B => {
                self.write_triangle_reg(addr, data);
            }

            // Noise
            0x400C => {
                self.n_halt = (data & 0x20) > 0;
                self.n_constant_volume = (data & 0x10) > 0;
                self.n_volume = data & 0x0F;
            }
            0x400E => {
                self.n_mode = (data & 0x80) > 0;
                self.n_timer_reload = NOISE_PERIOD_TABLE[(data & 0x0F) as usize];
            }
            0x400F => {
                if self.noise_enabled {
                    self.noise_length_counter = LENGTH_TABLE[((data >> 3) & 0x1F) as usize];
                }
            }

            // Channels Status Control
            0x4015 => {
                self.pulse1_enabled = (data & 0x01) > 0;
                self.pulse2_enabled = (data & 0x02) > 0;
                self.tri_enabled = (data & 0x04) > 0;
                self.noise_enabled = (data & 0x08) > 0;

                if !self.pulse1_enabled { self.p1_length_counter = 0; }
                if !self.pulse2_enabled { self.p2_length_counter = 0; }
                if !self.tri_enabled { self.tri_length_counter = 0; }
                if !self.noise_enabled { self.noise_length_counter = 0; }
            }

            // Frame Counter Mode
            0x4017 => {
                self.mode_5_step = (data & 0x80) > 0;
                self.irq_inhibit = (data & 0x40) > 0;
                if self.irq_inhibit {
                    self.frame_irq_flag = false;
                }
                self.frame_counter = 0;
            }
            _ => {}
        }
    }

    fn write_triangle_reg(&mut self, addr: u16, data: u8) {
        match addr {
            0x4008 => {
                self.tri_control_flag = (data & 0x80) > 0;
                self.tri_linear_reload = data & 0x7F;
            }
            0x400A => {
                self.tri_timer_reload = (self.tri_timer_reload & 0x0700) | (data as u16);
            }
            0x400B => {
                self.tri_timer_reload = (self.tri_timer_reload & 0x00FF) | (((data & 0x07) as u16) << 8);
                if self.tri_enabled {
                    self.tri_length_counter = LENGTH_TABLE[((data >> 3) & 0x1F) as usize];
                }
                self.tri_reload_flag = true;
            }
            _ => {}
        }
    }

    pub fn read_4015(&mut self) -> u8 {
        let mut status = 0;

        if self.p1_length_counter > 0 { status |= 0x01; }
        if self.p2_length_counter > 0 { status |= 0x02; }
        if self.tri_length_counter > 0 { status |= 0x04; }
        if self.noise_length_counter > 0 { status |= 0x08; }

        if self.frame_irq_flag { status |= 0x40; }
        
        // Reading $4015 acknowledges and clears the Frame Counter IRQ flag!
        self.frame_irq_flag = false;

        status
    }

    pub fn is_irq_asserted(&self) -> bool {
        self.frame_irq_flag && !self.irq_inhibit
    }

    pub fn write_p1_length(&mut self, data: u8) {
        if self.pulse1_enabled {
            self.p1_length_counter = LENGTH_TABLE[((data >> 3) & 0x1F) as usize];
        }
    }

    pub fn write_p2_length(&mut self, data: u8) {
        if self.pulse2_enabled {
            self.p2_length_counter = LENGTH_TABLE[((data >> 3) & 0x1F) as usize];
        }
    }

    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            // 1. Tick Core Timers
            if self.frame_counter % 2 == 0 {
                // Pulse 1
                if self.p1_timer == 0 {
                    self.p1_timer = self.p1_timer_reload;
                    self.p1_sequence_step = (self.p1_sequence_step + 1) & 7;
                } else {
                    self.p1_timer -= 1;
                }

                // Pulse 2
                if self.p2_timer == 0 {
                    self.p2_timer = self.p2_timer_reload;
                    self.p2_sequence_step = (self.p2_sequence_step + 1) & 7;
                } else {
                    self.p2_timer -= 1;
                }
            }

            // Triangle runs at CPU frequency
            if self.tri_timer == 0 {
                self.tri_timer = self.tri_timer_reload;
                if self.tri_length_counter > 0 && self.tri_linear_counter > 0 {
                    self.tri_sequence_step = (self.tri_sequence_step + 1) & 31;
                }
            } else {
                self.tri_timer -= 1;
            }

            // Noise runs at CPU frequency
            if self.n_timer == 0 {
                self.n_timer = self.n_timer_reload;
                let shift_bit = if self.n_mode { 6 } else { 1 };
                let feedback = (self.n_shift_register & 1) ^ ((self.n_shift_register >> shift_bit) & 1);
                self.n_shift_register = (self.n_shift_register >> 1) | (feedback << 14);
            } else {
                self.n_timer -= 1;
            }

            // 2. Mix Digital Outputs
            // Pulse 1
            if self.p1_length_counter > 0 && self.p1_timer_reload >= 8 {
                let current_bit = DUTY_TABLE[self.p1_duty_index as usize][self.p1_sequence_step as usize];
                self.p1_output = if current_bit > 0 { self.p1_volume as f32 } else { 0.0 };
            } else {
                self.p1_output = 0.0;
            }

            // Pulse 2
            if self.p2_length_counter > 0 && self.p2_timer_reload >= 8 {
                let current_bit = DUTY_TABLE[self.p2_duty_index as usize][self.p2_sequence_step as usize];
                self.p2_output = if current_bit > 0 { self.p2_volume as f32 } else { 0.0 };
            } else {
                self.p2_output = 0.0;
            }

            // Triangle
            let tri_output = if self.tri_length_counter > 0 && self.tri_linear_counter > 0 {
                TRI_SEQUENCE[self.tri_sequence_step as usize] as f32
            } else {
                0.0
            };

            // Noise
            let n_output = if self.noise_length_counter > 0 && (self.n_shift_register & 1) == 0 {
                self.n_volume as f32
            } else {
                0.0
            };

            // 3. Frame Sequencer Steps (~240 Hz updates)
            self.frame_counter += 1;
            let mut env_clock = false;
            let mut len_clock = false;

            if !self.mode_5_step {
                if self.frame_counter == 7457 || self.frame_counter == 22371 {
                    env_clock = true;
                } else if self.frame_counter == 14913 {
                    env_clock = true;
                    len_clock = true;
                } else if self.frame_counter >= 29830 {
                    env_clock = true;
                    len_clock = true;
                    if !self.irq_inhibit { self.frame_irq_flag = true; }
                    self.frame_counter = 0;
                }
            } else {
                if self.frame_counter == 7457 || self.frame_counter == 22371 {
                    env_clock = true;
                } else if self.frame_counter == 14913 {
                    env_clock = true;
                    len_clock = true;
                } else if self.frame_counter >= 37282 {
                    env_clock = true;
                    len_clock = true;
                    self.frame_counter = 0;
                }
            }

            if len_clock {
                if self.p1_length_counter > 0 && !self.p1_halt { self.p1_length_counter -= 1; }
                if self.p2_length_counter > 0 && !self.p2_halt { self.p2_length_counter -= 1; }
                if self.tri_length_counter > 0 && !self.tri_control_flag { self.tri_length_counter -= 1; }
                if self.noise_length_counter > 0 && !self.n_halt { self.noise_length_counter -= 1; }
            }

            if env_clock {
                // Triangle Linear Counter Processing
                if self.tri_reload_flag {
                    self.tri_linear_counter = self.tri_linear_reload;
                } else if self.tri_linear_counter > 0 {
                    self.tri_linear_counter -= 1;
                }
                if !self.tri_control_flag {
                    self.tri_reload_flag = false;
                }
            }

            // 4. Sample Extraction & Mixing Approximation
            self.sample_clock += 1.0;
            if self.sample_clock >= self.sample_rate_ratio {
                self.sample_clock -= self.sample_rate_ratio;
                
                // Approximate linear mixing weights
                let mixed = (self.p1_output + self.p2_output + (tri_output * 0.6) + (n_output * 0.4)) / 30.0;
                self.audio_buffer.push(mixed);
            }
        }
    }
}