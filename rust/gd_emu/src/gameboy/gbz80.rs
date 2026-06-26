#[derive(Clone, Copy, PartialEq)]
pub enum GbVariant {
    Dmg,
    Mgb,
    Cgb,
    Agb,
}

pub struct GbCpuConfig {
    pub variant: GbVariant,
    pub initial_a: u8,
    pub initial_f: u8,
    pub initial_bc: u16,
    pub initial_de: u16,
    pub initial_hl: u16,
    pub initial_sp: u16,
    pub supports_double_speed: bool,
}

impl GbCpuConfig {
    pub fn for_variant(variant: GbVariant) -> Self {
        match variant {
            GbVariant::Dmg => Self {
                variant, initial_a: 0x01, initial_f: 0xB0,
                initial_bc: 0x0013, initial_de: 0x00D8, initial_hl: 0x014D,
                initial_sp: 0xFFFE, supports_double_speed: false,
            },
            GbVariant::Mgb => Self {
                variant, initial_a: 0xFF, initial_f: 0xB0,
                initial_bc: 0x0013, initial_de: 0x00D8, initial_hl: 0x014D,
                initial_sp: 0xFFFE, supports_double_speed: false,
            },
            GbVariant::Cgb => Self {
                variant, initial_a: 0x11, initial_f: 0x80,
                initial_bc: 0x0000, initial_de: 0xFF56, initial_hl: 0x000D,
                initial_sp: 0xFFFE, supports_double_speed: true,
            },
            GbVariant::Agb => Self {
                variant, initial_a: 0x11, initial_f: 0x80,
                initial_bc: 0x0100, initial_de: 0xFF56, initial_hl: 0x000D,
                initial_sp: 0xFFFE, supports_double_speed: true,
            },
        }
    }
}