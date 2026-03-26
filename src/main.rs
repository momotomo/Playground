#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    rust_paint_foundation::native::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    rust_paint_foundation::web::start();
}
