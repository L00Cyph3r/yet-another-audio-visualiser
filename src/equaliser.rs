fn draw_equaliser(ui: &mut egui::Ui, bands: &[f32]) {
    let desired_size = egui::vec2(ui.available_width(), 250.0);

    let (rect, _) =
        ui.allocate_exact_size(desired_size, egui::Sense::hover());

    let painter = ui.painter_at(rect);

    let gap = 2.0;
    let bar_width =
        (rect.width() - gap * (bands.len() - 1) as f32)
            / bands.len() as f32;

    for (i, &level) in bands.iter().enumerate() {
        let x = rect.left() + i as f32 * (bar_width + gap);
        let height = level * rect.height();

        let bar = egui::Rect::from_min_size(
            egui::pos2(x, rect.bottom() - height),
            egui::vec2(bar_width, height),
        );

        painter.rect_filled(
            bar,
            2.0,
            ui.visuals().widgets.active.bg_fill,
        );
    }
}