use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const JAPANESE_UI_FONT_NAME: &str = "japanese_ui";
const JAPANESE_UI_FONT_BYTES: &[u8] =
    include_bytes!("../assets/fonts/ZenKakuGothicNew-Regular.ttf");

pub fn install_japanese_fonts(ctx: &egui::Context) {
    ctx.set_fonts(japanese_font_definitions());
}

fn japanese_font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        JAPANESE_UI_FONT_NAME.to_owned(),
        FontData::from_static(JAPANESE_UI_FONT_BYTES).into(),
    );

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(JAPANESE_UI_FONT_NAME.to_owned());
    }

    fonts
}

#[cfg(test)]
mod tests {
    use super::{JAPANESE_UI_FONT_NAME, japanese_font_definitions};
    use eframe::egui::FontFamily;

    #[test]
    fn japanese_font_is_added_to_proportional_and_monospace_fallbacks() {
        let fonts = japanese_font_definitions();

        let proportional = fonts
            .families
            .get(&FontFamily::Proportional)
            .expect("proportional family should exist");
        let monospace = fonts
            .families
            .get(&FontFamily::Monospace)
            .expect("monospace family should exist");

        assert!(
            proportional
                .iter()
                .any(|name| name == JAPANESE_UI_FONT_NAME)
        );
        assert!(monospace.iter().any(|name| name == JAPANESE_UI_FONT_NAME));
    }
}
