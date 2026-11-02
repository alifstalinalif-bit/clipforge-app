use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

pub const ACCENT: Color32 = Color32::from_rgb(45, 140, 235);
pub const BG: Color32 = Color32::from_rgb(27, 27, 27);
pub const PANEL: Color32 = Color32::from_rgb(35, 35, 35);
pub const HEADER: Color32 = Color32::from_rgb(46, 46, 46);
pub const LINE: Color32 = Color32::from_rgb(20, 20, 20);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 150, 150);
pub const VIDEO: Color32 = Color32::from_rgb(72, 118, 190);
pub const AUDIO: Color32 = Color32::from_rgb(52, 140, 104);
pub const IMAGE: Color32 = Color32::from_rgb(150, 100, 190);
pub const PLAYHEAD: Color32 = Color32::from_rgb(235, 70, 70);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(24, 24, 24);
    v.faint_bg_color = Color32::from_rgb(40, 40, 40);
    v.selection.bg_fill = ACCENT;
    v.widgets.inactive.bg_fill = Color32::from_rgb(58, 58, 58);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(58, 58, 58);
    v.widgets.hovered.bg_fill = Color32::from_rgb(75, 75, 75);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(75, 75, 75);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.inactive.rounding = egui::Rounding::same(6.0);
    v.widgets.hovered.rounding = egui::Rounding::same(6.0);
    v.widgets.active.rounding = egui::Rounding::same(6.0);
    v.window_rounding = egui::Rounding::same(10.0);
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.interact_size = Vec2::new(40.0, 32.0);
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 6.0);
    });
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .min_size(Vec2::new(0.0, 30.0)),
    )
}

pub fn panel_title(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().small().color(TEXT_DIM));
    ui.add_space(2.0);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    ToStart,
    ToEnd,
    StepBack,
    StepFwd,
}

/// Tombol ikon yang digambar sendiri (tidak tergantung font).
pub fn icon_button(ui: &mut Ui, rect: Rect, id: &str, icon: Icon, highlight: bool) -> Response {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let bg = if resp.is_pointer_button_down_on() {
        Color32::from_gray(85)
    } else if highlight {
        ACCENT
    } else if resp.hovered() {
        Color32::from_gray(64)
    } else {
        Color32::from_gray(50)
    };
    let p = ui.painter();
    p.rect_filled(rect, 8.0, bg);
    let c = rect.center();
    let s = rect.height().min(rect.width()) * 0.26;
    let col = Color32::WHITE;
    let tri_right = |cx: f32| {
        Shape::convex_polygon(
            vec![
                Pos2::new(cx - s * 0.7, c.y - s),
                Pos2::new(cx - s * 0.7, c.y + s),
                Pos2::new(cx + s * 0.9, c.y),
            ],
            col,
            Stroke::NONE,
        )
    };
    let tri_left = |cx: f32| {
        Shape::convex_polygon(
            vec![
                Pos2::new(cx + s * 0.7, c.y - s),
                Pos2::new(cx + s * 0.7, c.y + s),
                Pos2::new(cx - s * 0.9, c.y),
            ],
            col,
            Stroke::NONE,
        )
    };
    let bar = |cx: f32| {
        Shape::rect_filled(
            Rect::from_center_size(Pos2::new(cx, c.y), Vec2::new(s * 0.45, s * 2.0)),
            1.0,
            col,
        )
    };
    match icon {
        Icon::Play => {
            p.add(tri_right(c.x));
        }
        Icon::Pause => {
            p.add(bar(c.x - s * 0.5));
            p.add(bar(c.x + s * 0.5));
        }
        Icon::ToStart => {
            p.add(bar(c.x - s * 0.9));
            p.add(tri_left(c.x + s * 0.3));
        }
        Icon::ToEnd => {
            p.add(tri_right(c.x - s * 0.3));
            p.add(bar(c.x + s * 0.9));
        }
        Icon::StepBack => {
            p.add(tri_left(c.x));
        }
        Icon::StepFwd => {
            p.add(tri_right(c.x));
        }
    }
    resp
}
