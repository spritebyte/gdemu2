use godot::global::godot_print;
use crate::common::bus::AddressBus;
use crate::gameboy::mbc::Mbc;
use crate::gameboy::cartridge::Cartridge;
use crate::gameboy::gb_ppu::GbPPU;
use crate::gameboy::gameboy_apu::GbAPU;
use std::cell::{UnsafeCell, Cell};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct GameBoyBus {
    pub ram: [u8; 2048],
    pub cartridge: Cartridge,
    pub ppu: UnsafeCell<GbPPU>,
    pub apu: UnsafeCell<GbAPU>,
    // Input processing fields
    pub pad1_state: u8,
    pub pad1_shift_reg: Cell<u8>,
    pub pad_strobe: bool,
    pub dma_cycles: u32,
    pub total_cpu_cycles: u64,
}

unsafe impl Send for GameBoyBus {}
unsafe impl Sync for GameBoyBus {}

impl GameBoyBus {
    pub fn new(cartridge: Cartridge, system_frame_ready: Arc<AtomicBool>) -> Self {
        Self {
            ram: [0; 2048],                         // Zero out the 2KB of CPU RAM on startup
            ppu: UnsafeCell::new(NesPPU::new(system_frame_ready)),   // Initialize a fresh PPU 
            apu: UnsafeCell::new(NesAPU::new()),
            cartridge,                              // Inject the cartridge we loaded
            pad1_state: 0,
            pad1_shift_reg: Cell::new(0),
            pad_strobe: false,
            dma_cycles: 0,
            total_cpu_cycles: 0,
        }
    }
    pub fn get_sram(&self) -> Option<&[u8]> { self.cartridge.get_sram() }
    pub fn load_sram(&mut self, data: &[u8]) { self.cartridge.load_sram(data); }
    pub fn is_sram_dirty(&self) -> bool { self.cartridge.is_sram_dirty() }
    pub fn clear_sram_dirty(&mut self) { self.cartridge.clear_sram_dirty(); }
}

impl DmcMemoryReader for GameBoyBus {
    fn dmc_read(&self, addr: u16) -> u8 {
        self.read_byte(addr) // route through cartridge/mapper as normal
    }
}

impl AddressBus for GameBoyBus {
    fn is_nmi_line_asserted(&mut self) -> bool {
        self.ppu.get_mut().is_nmi_line_asserted()
    }

    fn is_irq_line_asserted(&mut self) -> bool {
        self.apu.get_mut().is_irq_asserted() || self.cartridge.mapper().is_irq_asserted()
    }

    fn read_byte(&self, addr: u16) -> u8 {

    }

    fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF01 => {
                self.serial_data_buffer = value;
            }
            0xFF02 => {
                self.serial_control = value;
                if (value & 0x80) != 0 {
                    // Bit 7 being set means a transfer was requested!
                    // This is the hook where your future Network/Link Cable component 
                    // will intercept execution and talk to the other emulator instance.
                    self.link_cable.initiate_transfer(self.serial_data_buffer, value);
                }
            }
            _ => {}
        }
    }
}