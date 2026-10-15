use bevy_egui::egui::{self, Color32, FontId, Stroke, Vec2};

const ACCENT: Color32 = Color32::from_rgb(107, 220, 207);

pub(crate) fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.global_style_mut(|style| {
        style.visuals.panel_fill = Color32::from_rgb(13, 20, 32);
        style.visuals.window_fill = Color32::from_rgb(17, 26, 40);
        style.visuals.extreme_bg_color = Color32::from_rgb(8, 14, 24);
        style.visuals.faint_bg_color = Color32::from_rgb(20, 31, 46);
        style.visuals.selection.bg_fill = Color32::from_rgb(28, 81, 83);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(24, 37, 54);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(35, 55, 73);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(35, 82, 86);
        style.visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0, Color32::from_rgb(33, 46, 64));
        style.spacing.item_spacing = Vec2::new(10.0, 9.0);
        style.spacing.button_padding = Vec2::new(12.0, 7.0);
        style.spacing.slider_width = 150.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(23.0));
    });
}
