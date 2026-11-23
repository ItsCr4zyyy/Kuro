//! Kuro core — doménová logika převzatá z NSC (server/src) a přenesená
//! na zařízení. Žádné FFI; to dělá samostatný crate `kuro-ffi`, aby šlo
//! všechno testovat obyčejným `cargo test` na desktopu.

pub mod error;
pub mod finder;
pub mod http;
pub mod matcher;
pub mod meta;
pub mod providers;
pub mod recommend;
pub mod storage;
pub mod text;
pub mod time;

mod rx;

pub use error::{Error, Result};
