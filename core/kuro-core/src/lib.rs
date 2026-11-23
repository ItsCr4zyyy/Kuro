//! Kuro core — doménová logika převzatá z NSC (server/src) a přenesená
//! na zařízení. Žádné FFI; to dělá samostatný crate `kuro-ffi`, aby šlo
//! všechno testovat obyčejným `cargo test` na desktopu.

pub mod matcher;
pub mod providers;
pub mod text;

mod rx;
