use godot::global::godot_print;
use crate::common::bus::AddressBus;
use crate::nes::mappers::Mapper;
use crate::nes::cartridge::Cartridge;
use crate::nes::nes_ppu::NesPPU;
use crate::nes::nes_apu::NesAPU;
use std::cell::{UnsafeCell, Cell};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub struct NesBus {
    pub ram: [u8; 2048],
    pub cartridge: Cartridge,
    pub ppu: UnsafeCell<NesPPU>,
    pub apu: UnsafeCell<NesAPU>,
    // Input processing fields
    pub pad1_state: u8,
    pub pad1_shift_reg: Cell<u8>,
    pub pad_strobe: bool,
}

unsafe impl Send for NesBus {}
unsafe impl Sync for NesBus {}

impl NesBus {
    pub fn new(cartridge: Cartridge, system_frame_ready: Arc<AtomicBool>) -> Self {
        Self {
            ram: [0; 2048],                         // Zero out the 2KB of CPU RAM on startup
            ppu: UnsafeCell::new(NesPPU::new(system_frame_ready)),   // Initialize a fresh PPU 
            apu: UnsafeCell::new(NesAPU::new()),
            cartridge,                              // Inject the cartridge we loaded
            pad1_state: 0,
            pad1_shift_reg: Cell::new(0),
            pad_strobe: false,
        }
    }
}

impl AddressBus for NesBus {
    fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr % 0x0800) as usize],
            0x2000..=0x3FFF => {
                let register = addr % 8;
                let mapper_ref = &*self.cartridge.mapper;
                let ppu_mut = unsafe { &mut *self.ppu.get() };
                ppu_mut.cpu_read_reg(mapper_ref, register)
            }
            0x4015 => {
                unsafe { (*self.apu.get()).read_4015() } // This returns the flag AND sets frame_irq_flag = false
            }
            0x4016 => {
                let shift_reg = self.pad1_shift_reg.get();
                let value = (shift_reg & 0x01) | 0x40;

                if !self.pad_strobe {
                    let next_shift = (shift_reg >> 1) | 0x80;
                    self.pad1_shift_reg.set(next_shift);
                }
                value
            }
            0x8000..=0xFFFF => self.cartridge.mapper.cpu_read(addr),
            _ => 0,
        }
    }

    fn write_byte(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr % 0x0800) as usize] = value,
            0x2000..=0x3FFF => {
                let register = addr % 8;
                let mapper_ref = &mut *self.cartridge.mapper;

                let ppu_mut = self.ppu.get_mut();
                ppu_mut.cpu_write_reg(mapper_ref, register, value);
            }
            0x4014 => {
                let page_start = (value as u16) << 8;
                let mut dma_buffer = [0u8; 256];

                for i in 0..256 {
                    dma_buffer[i] = self.read_byte(page_start + i as u16);
                }

                self.ppu.get_mut().write_oam_dma(&dma_buffer);
            }
            0x4016 => {
                self.pad_strobe = (value & 0x01) == 0x01;
                if self.pad_strobe {
                    self.pad1_shift_reg.set(self.pad1_state);
                }
            }
            0x4017 => {
                self.apu.get_mut().write_4017(value);
            }
            0x4000..=0x401F => { /* APU / IO registers - ignore for now */ }
            0x8000..=0xFFFF => self.cartridge.mapper.cpu_write(addr, value),
            _ => {}
        }
    }

    fn is_nmi_line_asserted(&mut self) -> bool {
         self.ppu.get_mut().is_nmi_line_asserted()
    }

    fn is_irq_line_asserted(&mut self) -> bool {
        self.apu.get_mut().is_irq_asserted() || self.cartridge.mapper.is_irq_asserted()
    }

    fn update_cycles(&mut self, cycles: u64) {
        // Forward the updated cycle counter to the cartridge's mapper
        self.cartridge.mapper.update_cycles(cycles);
    }
}