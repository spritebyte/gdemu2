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

const TRI_SEQUENCE: [u8;32] = [
    15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,0,
    0,1,2,3,4,5,6,7,8,9,10,11,12,13,15,15
];

const NOISE_PERIOD_TABLE: [u16;16]= [
	4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2032, 4064
];

pub struct NesAPU {
    frame_counter: u32,
    mode_5_step: bool,
    irq_inhibit: bool,
    frame_irq_flag: bool,
    
    // Pulse 1 Registers & Components
    p1_timer_reload: u16,      // Changed to u16 to hold 11-bit values
    p1_timer: u16,             // The actual running down-counter
    p1_sequence_step: u8,      // Progress through the 8-step duty cycle (0-7)
    p1_length_counter: u8,
    p1_duty_index: u8,
    p1_volume: u8,
    pulse1_enabled: bool,
    p1_halt: bool,
    // Pulse 2 Registers & Components
    p2_timer_reload: u16,      // Changed to u16 to hold 11-bit values
    p2_timer: u16,             // The actual running down-counter
    p2_sequence_step: u8,      // Progress through the 8-step duty cycle (0-7)
    p2_length_counter: u8,
    p2_duty_index: u8,
    p2_volume: u8,
    pulse2_enabled: bool,
    p2_halt: bool,
    // Triangle Channel State
    tri_enabled: bool,
    tri_timer_reload: u8,
    
    // Noise Channel Registers
    n_halt: bool,
    n_constant_volume: bool,
    n_volume: u8,
    n_mode: bool,
    n_timer_reload: u8,
    n_shift_register: u8,
    noise_enabled: bool,
    noise_length_counter: u8,

    // Envelopes
    p1_env_volume: u8,
    p1_env_divider: u8,
    p2_env_volume: u8,
    p2_env_divider: u8,
    n_env_volume: u8,
    n_env_divider: u8,

    // Audio Output Tracking
    audio_buffer: Vec<f32>,
    sample_clock: f32,
    sample_rate_ratio: f32,
    // Final calculated output sample
    p1_output: f32,            // Defined as f32 for output mixing
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
            tri_timer_reload: 0,
            n_halt: false,
            n_constant_volume: false,
            n_volume: 0,
            n_mode: false,
            n_timer_reload: 0,
            n_shift_register: 0,
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
            0x4000 => { 
                self.p1_duty_index = (data >> 6) & 0x03; 
                self.p1_halt = (data & 0x20) > 0;
                self.p1_volume = data & 0x0F;
            }
            0x4002 => { 
                // data supplies the low 8 bits of the timer reload
                self.p1_timer_reload = (self.p1_timer_reload & 0x0700) | (data as u16);
            }
            0x4003 => { 
                // data supplies the high 3 bits of the timer reload
                self.p1_timer_reload = (self.p1_timer_reload & 0x00FF) | (((data & 0x07) as u16) << 8); 
                
                // Writing to 0x4003 resets the wave sequencer back to the start
                self.p1_sequence_step = 0; 
                self.write_p1_length(data);
            }
            0x4004 => {
                self.p2_duty_index = (data >> 6) & 0x03;
            }
            0x4015 => {
                // Status register handles enabling/disabling channels
                self.pulse1_enabled = (data & 0x01) > 0;
                if !self.pulse1_enabled {
                    self.p1_length_counter = 0;
                }
            }
            _ => {}
        }
    }

    pub fn read_4015(&mut self) -> u8 {
        let mut status = 0;

        // 1. Report channel length counter statuses
        if self.p1_length_counter > 0 { status |= 0x01; }
        // Once you add other channels, uncomment these lines:
        // if self.p2_length_counter > 0 { status |= 0x02; }
        // if self.triangle_length_counter > 0 { status |= 0x04; }
        // if self.noise_length_counter > 0 { status |= 0x08; }
        // if self.dmc_bytes_remaining > 0 { status |= 0x10; }

        // 2. Report Interrupt statuses
        if self.frame_irq_flag { status |= 0x40; }
        // if self.dmc_irq_flag { status |= 0x80; }

        // 3. Reading $4015 automatically clears the frame IRQ flag
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

    // (Your existing write_4017, read_4015, and is_irq_asserted functions look solid!)

    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            // 1. Tick the internal APU timers
            // The pulse timer clocks down once every 2 CPU cycles
            if self.frame_counter % 2 == 0 {
                if self.p1_timer == 0 {
                    self.p1_timer = self.p1_timer_reload;
                    // Clock the sequence steps through 0 to 7 loops
                    self.p1_sequence_step = (self.p1_sequence_step + 1) & 7;
                } else {
                    self.p1_timer -= 1;
                }
            }

            // 2. Generate the current digital sample for Pulse 1
            // A channel only outputs sound if its length counter is > 0 and the timer is valid
            if self.p1_length_counter > 0 && self.p1_timer_reload >= 8 {
                let current_bit = DUTY_TABLE[self.p1_duty_index as usize][self.p1_sequence_step as usize];
                if current_bit > 0 {
                    self.p1_output = self.p1_volume as f32; // Returns a value from 0.0 to 15.0
                } else {
                    self.p1_output = 0.0;
                }
            } else {
                self.p1_output = 0.0;
            }

            // 3. Step the Frame Counter (Sequencer)
            self.frame_counter += 1;
            if !self.mode_5_step && self.frame_counter >= 29830 {
                if !self.irq_inhibit { self.frame_irq_flag = true; }
                self.frame_counter = 0;
            } else if self.mode_5_step && self.frame_counter >= 37282 {
                self.frame_counter = 0;
            }
        }
    }
}