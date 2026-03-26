use crate::PaintApp;

pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rust Paint Foundation")
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([960.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Paint Foundation",
        native_options,
        Box::new(|creation_context| Ok(Box::new(PaintApp::new(creation_context)))),
    )
}
