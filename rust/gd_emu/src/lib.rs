use godot::prelude::*;

struct GDEmulatorExtension;
#[gdextension]
unsafe impl ExtensionLibrary for GDEmulatorExtension {}


mod nes;
pub mod common;

