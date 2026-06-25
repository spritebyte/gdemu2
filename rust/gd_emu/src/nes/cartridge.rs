use crate::nes::mappers::Mapper;

pub struct Cartridge {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mapper: Box<dyn Mapper>, // Dynamic trait object
    pub mapper_id: u8,
    pub base_filename: String,
    pub has_battery: bool,
}

impl Cartridge {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mapper: Box<dyn Mapper>, base_name:String) -> Cartridge {
        Self {
            prg_rom,
            chr_rom,
            mapper,
            mapper_id: 0,
            base_filename: base_name,
            has_battery: false,
        }
    }
    pub fn get_sram(&self) -> Option<&[u8]> { self.mapper.get_sram() }
    pub fn load_sram(&mut self, data: &[u8]) { self.mapper.load_sram(data); }
    pub fn is_sram_dirty(&self) -> bool { self.mapper.is_sram_dirty() }
    pub fn clear_sram_dirty(&mut self) { self.mapper.clear_sram_dirty(); }
}