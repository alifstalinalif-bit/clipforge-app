use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

pub const FPS: f64 = 30.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Video,
    Audio,
}

#[derive(Clone)]
pub struct Clip {
    pub name: String,
    pub start: f64, // detik di timeline
    pub len: f64,   // durasi detik
}

pub struct Track {
    pub name: String,
    pub kind: Kind,
    pub clips: Vec<Clip>,
    pub muted: bool,
}

pub struct Timeline {
    pub tracks: Vec<Track>,
    pub playhead: f64,
    pub zoom: f32, // pixel per detik
    pub selected: Option<(usize, usize)>,
}

impl Timeline {
    pub fn demo() -> Self {
        let clip = |n: &str, s: f64, l: f64| Clip { name: n.into(), start: s, len: l };
        let track = |n: &str, k: Kind, c: Vec<Clip>| Track { name: n.into(), kind: k, clips: c, muted: false };
        Self {
            tracks: vec![
                track("V3", Kind::Video, vec![clip("Title.png", 2.0, 4.0)]),
                track("V2", Kind::Video, vec![clip("B-roll.mp4", 4.0, 6.0)]),
                track("V1", Kind::Video, vec![clip("Intro.mp4", 0.0, 8.0), clip("Main.mp4", 8.0, 12.0)]),
                track("A1", Kind::Audio, vec![clip("Intro.mp4", 0.0, 8.0), clip("Main.mp4", 8.0, 12.0)]),
                track("A2", Kind::Audio, vec![clip("Music.mp3", 0.0, 20.0)]),
            ],
            playhead: 0.0,
            zoom: 50.0,
            selected: None,
        }
    }

    pub fn duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.start + c.len)
            .fold(0.0, f64::max)
    }

    pub fn timecode(t: f64) -> String {
        let total_frames = (t * FPS).round() as u64;
        let f = total_frames % FPS as u64;
        let s = (total_frames / FPS as u64) % 60;
        let m = (total_frames / FPS as u64 / 60) % 60;
        let h = total_frames / FPS as u64 / 3600;
        format!("{h:02}:{m:02}:{s:02}:{f:02}")
    }

    /// Razor: potong semua clip yang dilewati playhead.
    pub fn split_at_playhead(&mut self) {
        let t = self.playhead;
        for track in &mut self.tracks {
            let mut extra = Vec::new();
            for c in &mut track.clips {
                if t > c.start + 0.01 && t < c.start + c.len - 0.01 {
                    let right = Clip { name: c.name.clone(), start: t, len: c.start + c.len - t };
                    c.len = t - c.start;
                    extra.push(right);
                }
            }
            track.clips.extend(extra);
        }
        self.selected = None;
    }

    pub fn delete_selected(&mut self) {
        if let Some((ti, ci)) = self.selected.take() {
            if ti < self.tracks.len() && ci < self.tracks[ti].clips.len() {
                self.tracks[ti].clips.remove(ci);
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("✂ Razor (C)").clicked() {
                self.split_at_playhead();
            }
            if ui.button("🗑 Delete").clicked() {
                self.delete_selected();
            }
            ui.separator();
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut self.zoom, 10.0..=200.0).show_value(false));
        });

        let header_w = 80.0;
        let ruler_h = 22.0;
        let track_h = 36.0;
        let zoom = self.zoom;
        let n = self.tracks.len();

        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let w = ((self.duration() + 10.0) as f32 * zoom + header_w).max(ui.available_width());
            let h = ruler_h + track_h * n as f32;
            let (rect, bg) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
            let p = ui.painter_at(rect);
            let x_of = |t: f64| rect.left() + header_w + t as f32 * zoom;

            p.rect_filled(rect, 0.0, Color32::from_rgb(35, 35, 35));

            // Ruler
            p.rect_filled(
                Rect::from_min_size(rect.min, Vec2::new(w, ruler_h)),
                0.0,
                Color32::from_rgb(45, 45, 45),
            );
            let step = if zoom > 80.0 { 1.0 } else if zoom > 30.0 { 5.0 } else { 10.0 };
            let mut t = 0.0;
            while x_of(t) < rect.right() {
                let x = x_of(t);
                p.line_segment(
                    [Pos2::new(x, rect.top() + 12.0), Pos2::new(x, rect.top() + ruler_h)],
                    Stroke::new(1.0, Color32::GRAY),
                );
                p.text(
                    Pos2::new(x + 3.0, rect.top() + 1.0),
                    egui::Align2::LEFT_TOP,
                    Self::timecode(t),
                    egui::FontId::monospace(9.0),
                    Color32::LIGHT_GRAY,
                );
                t += step;
            }

            // Scrub di ruler
            if bg.clicked() || bg.dragged() {
                if let Some(pos) = bg.interact_pointer_pos() {
                    if pos.y - rect.top() < ruler_h {
                        self.playhead = (((pos.x - rect.left() - header_w) / zoom).max(0.0)) as f64;
                    } else if bg.clicked() {
                        self.selected = None;
                    }
                }
            }

            // Tracks
            for ti in 0..n {
                let y = rect.top() + ruler_h + ti as f32 * track_h;
                let (kind, name) = (self.tracks[ti].kind, self.tracks[ti].name.clone());
                p.rect_filled(
                    Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(header_w, track_h)),
                    0.0,
                    Color32::from_rgb(50, 50, 50),
                );
                p.text(
                    Pos2::new(rect.left() + 8.0, y + track_h / 2.0),
                    egui::Align2::LEFT_CENTER,
                    name,
                    egui::FontId::proportional(13.0),
                    Color32::WHITE,
                );
                p.line_segment(
                    [Pos2::new(rect.left(), y + track_h), Pos2::new(rect.right(), y + track_h)],
                    Stroke::new(1.0, Color32::from_rgb(25, 25, 25)),
                );

                for ci in 0..self.tracks[ti].clips.len() {
                    let c = self.tracks[ti].clips[ci].clone();
                    let r = Rect::from_min_size(
                        Pos2::new(x_of(c.start), y + 2.0),
                        Vec2::new(c.len as f32 * zoom, track_h - 4.0),
                    );
                    let resp = ui.interact(r, ui.id().with((ti, ci)), Sense::click_and_drag());
                    if resp.clicked() || resp.drag_started() {
                        self.selected = Some((ti, ci));
                    }
                    if resp.dragged() {
                        let dx = resp.drag_delta().x / zoom;
                        let cl = &mut self.tracks[ti].clips[ci];
                        cl.start = (cl.start + dx as f64).max(0.0);
                    }
                    let base = match kind {
                        Kind::Video => Color32::from_rgb(78, 120, 190),
                        Kind::Audio => Color32::from_rgb(60, 150, 110),
                    };
                    let sel = self.selected == Some((ti, ci));
                    p.rect_filled(r, 3.0, base);
                    p.rect_stroke(
                        r,
                        3.0,
                        Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { Color32::WHITE } else { Color32::BLACK }),
                    );
                    p.text(
                        r.left_center() + Vec2::new(5.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &c.name,
                        egui::FontId::proportional(11.0),
                        Color32::WHITE,
                    );
                }
            }

            // Playhead
            let px = x_of(self.playhead);
            p.line_segment(
                [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
                Stroke::new(1.5, Color32::from_rgb(230, 60, 60)),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(px - 6.0, rect.top()),
                    Pos2::new(px + 6.0, rect.top()),
                    Pos2::new(px, rect.top() + 11.0),
                ],
                Color32::from_rgb(230, 60, 60),
                Stroke::NONE,
            ));
        });
    }
}
