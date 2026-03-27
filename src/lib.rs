//! Shared application modules for native and web targets.

pub mod app;
pub mod canvas;
pub mod fonts;
pub mod model;
pub mod render;
pub mod storage;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub use app::PaintApp;
