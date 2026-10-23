//! Godot 4.7 GDExtension for on-the-drop. Thin wrapper: all logic lives in
//! the `on-the-drop` crate.

use godot::prelude::*;

struct OnTheDropExtension;

#[gdextension]
unsafe impl ExtensionLibrary for OnTheDropExtension {}

/// Song analysis entry point. All functions are static:
/// `OnTheDrop.version()`, `OnTheDrop.band_energy(samples, rate, hop)`.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct OnTheDrop {
    base: Base<RefCounted>,
}

#[godot_api]
impl OnTheDrop {
    /// Version of the Rust core library.
    #[func]
    fn version() -> GString {
        GString::from(on_the_drop::version())
    }

    /// Bass-band energy per hop of mono samples. Empty if rate or hop <= 0.
    #[func]
    fn band_energy(samples: PackedFloat32Array, rate: i64, hop: i64) -> PackedFloat32Array {
        if rate <= 0 || rate > u32::MAX as i64 || hop <= 0 {
            return PackedFloat32Array::new();
        }
        let out = on_the_drop::band_energy(samples.as_slice(), rate as u32, hop as usize);
        PackedFloat32Array::from(out.as_slice())
    }
}
