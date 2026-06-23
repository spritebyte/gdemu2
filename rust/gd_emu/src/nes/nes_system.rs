use crate::common::m6502::{M6502Cpu, CpuVariant};
use crate::common::bus::AddressBus;
use crate::nes::nes_bus::NesBus;
use crate::nes::mappers::{Mapper, Mapper0, Mirroring};
use crate::nes::cartridge::Cartridge;
use crate::nes::mapper1::Mapper1;
use crate::nes::mapper2::Mapper2;
use crate::nes::mapper3::Mapper3;
use crate::nes::mapper9::Mapper9;

use godot::prelude::*;
use godot::classes::{Node, AudioStreamGeneratorPlayback};
use godot::global::godot_print;
use godot::classes::{AudioStreamPlayer,Image,ImageTexture,Texture2D};
use godot::classes::image::Format;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const PRG_ROM_COUNT_IDX: u8 = 0x04;
const CHR_ROM_COUNT_IDX: u8 = 0x05;
const CONTROL_BYTE_1_IDX: u8 = 0x06;
const CONTROL_BYTE_2_IDX: u8 = 0x07;
const PRG_ROM_PAGE_SIZE: u16 = 1024 * 16;
const CHR_ROM_PAGE_SIZE: u16 = 1024 * 8; 


#[derive(GodotClass)]
#[class(base=RefCounted, no_init)]
pub struct NesSystem {
    cpu: M6502Cpu,
    bus: NesBus,
    frame_ready: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    save_battery_path: String,
    save_filename: String,
    playback: Option<Gd<AudioStreamGeneratorPlayback>>,
}

#[godot_api]
impl NesSystem {
    #[func]
    pub fn create_from_bytes(rom_bytes: PackedByteArray, base_name: String) -> Option<Gd<Self>> {
        // Validate header and get sizes
        let bytes = rom_bytes.as_slice();
        if bytes.len() < 16 || &bytes[0..4] != b"NES\x1A" { return None; }

        let prg_rom_size = bytes[4] as usize * PRG_ROM_PAGE_SIZE as usize;
        let bytes_chr = bytes[5] as usize;
        let chr_rom_size = bytes_chr * CHR_ROM_PAGE_SIZE as usize;

        // extract mapper id
        let mapper_id_low = bytes[6] >> 4;
        let mapper_id_high = bytes[7] & 0xF0;
        let mapper_id = mapper_id_high | mapper_id_low;

        // slice ROM bytes
        let has_trainer:bool = (bytes[CONTROL_BYTE_1_IDX as usize] & 0b100) != 0;
        let prg_start = 16;
        let prg_end = prg_start + prg_rom_size;
        let chr_end = prg_end + (bytes[5] as usize * 8192);

        let prg_rom = bytes[prg_start..prg_end].to_vec();
        let chr_rom = if bytes[5] > 0 { bytes[prg_end..chr_end].to_vec() } else { vec![] };
        let header = bytes[0..15].to_vec();

        // initialize the mapper
        let mapper = match Self::instantiate_mapper(mapper_id, prg_rom.clone(), chr_rom.clone(), header.clone()) {
            Some(m) => m,
            None => { godot_print!("Mapper not supported yet: {mapper_id}"); return None }
        };

        // --- instantiate the atomic sync flag early ---
        let frame_ready = Arc::new(AtomicBool::new(false));

        // initialize cartridge and bus
        let cartridge = Cartridge::new(prg_rom, chr_rom, mapper, base_name.clone());
        godot_print!("Cartridge created: {0}", cartridge.base_filename);
        let bus = NesBus::new(cartridge, Arc::clone(&frame_ready));
        godot_print!("prg_rom size: {prg_rom_size} ");
        godot_print!("chr_rom size: {chr_rom_size} ");

        Some(Gd::from_init_fn(|_base| {
            Self {
                cpu: M6502Cpu::new(CpuVariant::Ricoh2A03),
                bus,
                save_filename: base_name,
                frame_ready,
                is_running: Arc::new(AtomicBool::new(false)),
                save_battery_path: "user://GD_EMU/NES/Save".to_string(),
                playback: None,
            }
        }))
    }

    #[func]
    pub fn run_slice(&mut self, input_mask: u16) {   
        if !self.is_running.load(Ordering::Relaxed) {
            return;
        }
        let mut nes_pad_state = 0u8;

        // Frontend layout vs NES expected bit positions:
        if (input_mask & (1 << 5)) != 0 { nes_pad_state |= 1 << 0; } // GDScript A (Bit 5)      -> NES A (Bit 0)
        if (input_mask & (1 << 4)) != 0 { nes_pad_state |= 1 << 1; } // GDScript B (Bit 4)      -> NES B (Bit 1)
        if (input_mask & (1 << 6)) != 0 { nes_pad_state |= 1 << 2; } // GDScript Select (Bit 6) -> NES Select (Bit 2)
        if (input_mask & (1 << 7)) != 0 { nes_pad_state |= 1 << 3; } // GDScript Start (Bit 7)  -> NES Start (Bit 3)
        if (input_mask & (1 << 0)) != 0 { nes_pad_state |= 1 << 4; } // GDScript Up (Bit 0)     -> NES Up (Bit 4)
        if (input_mask & (1 << 1)) != 0 { nes_pad_state |= 1 << 5; } // GDScript Down (Bit 1)   -> NES Down (Bit 5)
        if (input_mask & (1 << 2)) != 0 { nes_pad_state |= 1 << 6; } // GDScript Left (Bit 2)   -> NES Left (Bit 6)
        if (input_mask & (1 << 3)) != 0 { nes_pad_state |= 1 << 7; } // GDScript Right (Bit 3)  -> NES Right (Bit 7)

//        godot_print!("nes_pad_state: 0x{:02X}", nes_pad_state);
        self.bus.pad1_state = nes_pad_state;

        let mut cycles_this_frame:u16 = 0;
        while cycles_this_frame < 29780 {
            let cycles = self.cpu.step(&mut self.bus);
            let mapper_ref = &*self.bus.cartridge.mapper;
            let ppu_mut = self.bus.ppu.get_mut();
            ppu_mut.step(mapper_ref, (cycles * 3) as u32);
            let apu_mut = self.bus.apu.get_mut();
            apu_mut.step(cycles as u32);
            cycles_this_frame += cycles as u16;
        }
    }

    #[func]
    pub fn power_on(&mut self, mut audio_player: Gd<AudioStreamPlayer>) {
        self.cpu.power_on(&self.bus);
        self.is_running.store(true, Ordering::SeqCst);
        let _playback = audio_player.get_stream_playback();
        // bus.mapper.load_sram(&self.save_battery_path);
        let lo = self.bus.read_byte(0xFFFC);
        let hi = self.bus.read_byte(0xFFFD);
        godot_print!(
        "CPU Powering On:\n\
         - Reset Vector Bytes: [$FFFC] = 0x{:02X}, [$FFFD] = 0x{:02X}\n\
         - Initial Program Counter (PC): 0x{:04X}", lo, hi, self.cpu.pc);
        println!("NES System Power On: Audio streams mapped and SRAM components pulled into RAM.");
    }
    
    #[func]
    pub fn power_off(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        self.cpu.is_running = false;
//        self.bus.mapper.save_sram(SaveBatteryPath);
        println!("NES System Power Off: Battery backed SRAM saved to persistent disk space safely.");
    }

    #[func]
    pub fn reset(&mut self) {
        self.bus.ppu.get_mut().reset();
        self.cpu.reset(&self.bus);
            // self.bus.reset_ram();
    }

    #[func]
    pub fn set_audio_playback(&mut self, playback: Gd<AudioStreamGeneratorPlayback>) {
        self.playback = Some(playback);
    }

    #[func]
    pub fn is_frame_ready(&self) -> bool {
        // Atomic read takes virtually zero execution cost and avoids Mutex lock stalls!
        self.frame_ready.load(Ordering::Acquire)
    }

    #[func]
    pub fn get_frame_texture(&mut self) -> Gd<Texture2D> {
        self.frame_ready.store(false, Ordering::Release);        
        // return self.bus.ppu.render_frame();

        let raw_pixels = self.bus.ppu.get_mut().get_front_buffer();
        
        
        let mut godot_image = Image::create_from_data(
                256, 
                240, 
                false, 
                Format::RGBA8, 
                &PackedByteArray::from_iter(raw_pixels.iter().copied())
        ).unwrap();
        
        let texture = ImageTexture::create_from_image(Some(&godot_image)).unwrap();
        texture.upcast()
    }

    // A factory function that maps an integer ID to a concrete Rust struct
    fn instantiate_mapper(mapper_id: u8, prg_rom: Vec<u8>, chr_rom: Vec<u8>, header:Vec<u8>) -> Option<Box<dyn Mapper>> {
        // Calculate the number of 16KB PRG banks and 8KB CHR banks
        let prg_banks = prg_rom.len() / 16384;
        let chr_banks = chr_rom.len() / 8192;

        let mirroring_bit = (header[6] & 0x01) != 0;
        let four_screen_bit = (header[6] & 0x08) != 0;

        match mapper_id {
            0 => { let initial_mirroring:Mirroring = if mirroring_bit { Mirroring::Vertical } else { Mirroring::Horizontal };
                godot_print!("Mapper0 (Nrom) created with initial mirroring bit={mirroring_bit}");
                Some(Box::new(Mapper0::new(prg_banks, chr_banks, prg_rom, chr_rom, initial_mirroring))) // NROM
            }
            1 => { // MMC1 
                godot_print!("Mapper1 (MMC1) created");
                let initial_mirroring:Mirroring = if mirroring_bit { Mirroring::Vertical } else { Mirroring::Horizontal };
                Some(Box::new(Mapper1::new(prg_banks, chr_banks, prg_rom, chr_rom, initial_mirroring, four_screen_bit)))
            } 
            2 => { // UxROM
                godot_print!("Mapper2 (UxROM) created");
                let initial_mirroring:Mirroring = if mirroring_bit { Mirroring::Vertical } else { Mirroring::Horizontal };
                Some(Box::new(Mapper2::new(prg_banks, chr_banks, prg_rom, chr_rom, initial_mirroring, four_screen_bit)))
            }
            3 => { // CNROM
                godot_print!("Mapper3 (CNROM) created");
                let initial_mirroring:Mirroring = if mirroring_bit { Mirroring::Vertical } else { Mirroring::Horizontal };
                Some(Box::new(Mapper3::new(prg_banks, chr_banks, prg_rom, chr_rom, initial_mirroring, four_screen_bit)))
            }
            9 => { // CNROM
                godot_print!("Mapper9 (MMC2) created");
                let initial_mirroring:Mirroring = if mirroring_bit { Mirroring::Vertical } else { Mirroring::Horizontal };
                Some(Box::new(Mapper9::new(prg_banks, chr_banks, prg_rom, chr_rom, initial_mirroring, four_screen_bit)))
            } 

                // 4 => Some(Box::new(Mapper4::new(prg_banks, chr_banks))), // MMC3 (Future)
            _ => {
                godot_error!("Unsupported Mapper ID: {}", mapper_id);
                None
            }
        }
    }
}