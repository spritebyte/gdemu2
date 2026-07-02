use godot::prelude::*;
use godot::classes::{Node, AudioStreamGeneratorPlayback};
use godot::global::godot_print;
use godot::classes::{AudioStreamPlayer,Image,ImageTexture,Texture2D};
use godot::classes::image::Format;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(GodotClass)]
#[class(base=RefCounted, no_init)]
struct GameBoySystem {
    cpu: GameBoyCpu,
    bus: GameBoyBus,
    frame_ready: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    save_battery_path: String,
    save_filename: String,
    sys_display: Gd<SystemDisplayInfo>,
    playback: Option<Gd<AudioStreamGeneratorPlayback>>,
    cached_image: Option<Gd<Image>>,
    cached_texture: Option<Gd<ImageTexture>>,
    total_t_cycles: u64,
}

impl GameBoySystem {
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

        let prg_start = 0;
        let prg_end = prg_start + prg_rom_size;
        let chr_end = prg_end + (bytes[5] as usize * 8192);

        let prg_rom = bytes[prg_start..prg_end].to_vec();
        let chr_rom = if bytes[5] > 0 { bytes[prg_end..chr_end].to_vec() } else { vec![] };
        let header = bytes[0..15].to_vec();

        // initialize the mapper
        let mbc = match Self::instantiate_mbc(mbc_id, prg_rom.clone(), chr_rom.clone(), header.clone()) {
            Some(m) => m,
            None => { godot_print!("MBC not supported yet: {mbc_id}"); return None }
        };

        // --- instantiate the atomic sync flag early ---
        let frame_ready = Arc::new(AtomicBool::new(false));

        // initialize cartridge and bus
        let cartridge = Cartridge::new(prg_rom, chr_rom, mbc, base_name.clone());
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
                sys_display: Gd::from_object(SystemDisplayInfo::new()),
                cached_image: None,
                cached_texture: None,
            }
        }))
    }

    #[func]
    pub fn get_display_info(&self) -> Gd<SystemDisplayInfo> {
        self.sys_display.clone()
    }

    pub fn tick(&mut self) {
        // 1. Tell the CPU to advance by exactly 1 M-cycle step
        self.cpu.step_one_m_cycle(&mut self.bus);

        // 2. Determine how many T-cycles actually passed on the system bus
        let cycles_passed = if self.bus.is_double_speed_active() {
            2 // In CGB Double Speed, a CPU M-cycle takes only 2 T-cycles
        } else {
            4 // In normal speed, a CPU M-cycle takes 4 T-cycles
        }

        // 3. Accumulate global time and tick the rest of the physical hardware
        self.total_t_cycles += cycles_passed;
    
        self.bus.timer.tick(cycles_passed);
        self.bus.ppu.tick(cycles_passed);
        self.bus.apu.tick(cycles_passed);
    }
}