use crate::PaintApp;
use wasm_bindgen::JsCast;

pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        let web_options = eframe::WebOptions::default();
        let canvas = canvas_element();

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|creation_context| Ok(Box::new(PaintApp::new(creation_context)))),
            )
            .await;

        if result.is_ok() {
            remove_loading_screen();
        } else if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            if let Some(loading) = document.get_element_by_id("loading") {
                loading.set_inner_html("Failed to start the app. Check the browser console.");
            }
        }

        result.expect("failed to start Rust Paint Foundation");
    });
}

fn canvas_element() -> web_sys::HtmlCanvasElement {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("paint_canvas"))
        .expect("missing #paint_canvas element")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("#paint_canvas is not a canvas element")
}

fn remove_loading_screen() {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(loading) = document.get_element_by_id("loading") {
            loading.remove();
        }
    }
}
