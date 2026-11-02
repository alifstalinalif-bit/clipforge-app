mod browser;
mod platform;
mod theme;
mod timeline;

use browser::Browser;
use egui::{
    Align2, Color32, FontId, Frame, Margin, Pos2, Rect, RichText, ScrollArea, Sense, TextureHandle,
    TextureOptions, Vec2,
};
use platform::{AudioClip, VideoReq};
use std::time::Duration;
use theme::*;
use timeline::{Kind, MediaItem, Timeline, FPS};

#[derive(Clone, Copy, PartialEq)]
enum Drawer {
    Media,
    Edit,
    Closed,
}

pub struct ClipForge {
    tl: Timeline,
    media: Vec<MediaItem>,
    browser: Browser,
    playing: bool,
    drawer: Drawer,
    status: String,
    status_until: f64,
    last_immersive: f64,
    tex: Option<TextureHandle>,
    tex_path: Option<String>,
    last_req: Option<(String, i64)>,
}

impl ClipForge {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        platform::set_immersive();
        platform::av_init(&cc.egui_ctx);
        Self {
            tl: Timeline::new(),
            media: vec![],
            browser: Browser::new(),
            playing: false,
            drawer: Drawer::Media,
            status: String::new(),
            status_until: 0.0,
            last_immersive: 0.0,
            tex: None,
            tex_path: None,
            last_req: None,
        }
    }

    fn say(&mut self, ctx: &egui::Context, msg: String) {
        self.status = msg;
        self.status_until = ctx.input(|i| i.time) + 3.0;
    }

    fn fmt_dur(d: f64) -> String {
        let s = d.round() as u64;
        format!("{}:{:02}", s / 60, s % 60)
    }

    fn toggle_play(&mut self) {
        let dur = self.tl.duration();
        if dur <= 0.0 {
            return;
        }
        if !self.playing && self.tl.playhead >= dur - 0.01 {
            self.tl.playhead = 0.0;
        }
        self.playing = !self.playing;
    }

    /// Clip visual teratas di posisi playhead: (path, jenis, waktu di sumber, opacity).
    fn visual_at(&self) -> Option<(String, Kind, f64, f32)> {
        let t = self.tl.playhead;
        for tr in self.tl.tracks.iter().filter(|tr| !tr.audio) {
            for c in tr.clips.iter().rev() {
                if t >= c.start && t < c.end() {
                    return Some((c.path.clone(), c.kind, c.src_in + (t - c.start), c.opacity));
                }
            }
        }
        None
    }

    /// Hubungkan timeline ke decoder video dan pemutar audio.
    fn pump_av(&mut self, ctx: &egui::Context) {
        // frame video baru dari worker
        if let Some(f) = platform::video_take() {
            let img = egui::ColorImage::from_rgba_unmultiplied(f.size, &f.rgba);
            match &mut self.tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture("monitor", img, TextureOptions::LINEAR)),
            }
            self.tex_path = Some(f.path);
        }

        // minta frame untuk posisi playhead sekarang
        match self.visual_at() {
            Some((path, kind, st, _)) => {
                let is_img = kind == Kind::Image;
                let q = if is_img { 0 } else { (st * FPS).round() as i64 };
                let key = (path.clone(), q);
                if self.last_req.as_ref() != Some(&key) {
                    self.last_req = Some(key);
                    platform::video_request(VideoReq { path, image: is_img, t: st });
                }
            }
            None => self.last_req = None,
        }

        // audio: clip yang sedang aktif + yang akan mulai < 1 detik lagi
        let t = self.tl.playhead;
        let mut ac: Vec<AudioClip> = vec![];
        for tr in self.tl.tracks.iter().filter(|tr| tr.audio) {
            for c in &tr.clips {
                let volume = c.volume;
                if t >= c.start && t < c.end() {
                    ac.push(AudioClip { id: c.id, path: c.path.clone(), pos: c.src_in + (t - c.start), volume });
                } else if c.start > t && c.start - t < 1.0 {
                    ac.push(AudioClip { id: c.id, path: c.path.clone(), pos: -(c.start - t), volume });
                }
            }
        }
        platform::audio_sync(self.playing, ac);
    }

    // ---------------- UI ----------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(38.0)
            .frame(Frame::none().fill(HEADER).inner_margin(Margin::symmetric(8.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("CLIP FORGE").strong().size(14.0).color(ACCENT));
                    ui.separator();
                    if ui.selectable_label(self.drawer == Drawer::Media, "Media").clicked() {
                        self.drawer = if self.drawer == Drawer::Media { Drawer::Closed } else { Drawer::Media };
                    }
                    if ui.selectable_label(self.drawer == Drawer::Edit, "Edit").clicked() {
                        self.drawer = if self.drawer == Drawer::Edit { Drawer::Closed } else { Drawer::Edit };
                    }
                    ui.separator();
                    if primary_button(ui, "+ Import").clicked() {
                        self.browser.open();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.monospace(Timeline::timecode(self.tl.duration()));
                        if !self.status.is_empty() {
                            ui.separator();
                            ui.label(RichText::new(&self.status).color(Color32::from_rgb(255, 210, 90)));
                        }
                    });
                });
            });
    }

    fn drawer_panel(&mut self, ctx: &egui::Context) {
        if self.drawer == Drawer::Closed {
            return;
        }
        let w = (ctx.screen_rect().width() * 0.26).clamp(190.0, 280.0);
        egui::SidePanel::left("drawer")
            .exact_width(w)
            .resizable(false)
            .frame(Frame::none().fill(PANEL).inner_margin(Margin::same(8.0)))
            .show(ctx, |ui| match self.drawer {
                Drawer::Media => self.media_ui(ui),
                Drawer::Edit => self.inspector_ui(ui),
                Drawer::Closed => {}
            });
    }

    fn media_ui(&mut self, ui: &mut egui::Ui) {
        panel_title(ui, "MEDIA");
        if self.media.is_empty() {
            ui.add_space(6.0);
            ui.weak("Belum ada media.");
            ui.weak("Tap + Import di atas.");
        }
        let mut add: Option<usize> = None;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (i, m) in self.media.iter().enumerate() {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), Sense::hover());
                let add_rect = Rect::from_min_size(Pos2::new(rect.right() - 42.0, rect.top()), Vec2::new(42.0, 40.0));
                let add_resp = ui.interact(add_rect, ui.id().with(("add", i)), Sense::click());
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 6.0, Color32::from_gray(44));
                p.rect_filled(Rect::from_min_size(rect.left_top(), Vec2::new(5.0, rect.height())), 2.0, m.kind.color());
                let text_clip = Rect::from_min_max(rect.min, Pos2::new(add_rect.left() - 2.0, rect.max.y));
                let tp = p.with_clip_rect(text_clip);
                tp.text(
                    Pos2::new(rect.left() + 13.0, rect.top() + 13.0),
                    Align2::LEFT_CENTER,
                    &m.name,
                    FontId::proportional(12.5),
                    Color32::WHITE,
                );
                let sub = if m.kind == Kind::Image {
                    "Gambar".to_string()
                } else {
                    format!("{}  {}", m.kind.label(), Self::fmt_dur(m.duration))
                };
                tp.text(
                    Pos2::new(rect.left() + 13.0, rect.top() + 29.0),
                    Align2::LEFT_CENTER,
                    sub,
                    FontId::proportional(10.0),
                    TEXT_DIM,
                );
                let br = add_rect.shrink2(Vec2::new(5.0, 6.0));
                let col = if add_resp.is_pointer_button_down_on() { Color32::from_rgb(30, 100, 180) } else { ACCENT };
                p.rect_filled(br, 6.0, col);
                p.text(br.center(), Align2::CENTER_CENTER, "+", FontId::proportional(18.0), Color32::WHITE);
                if add_resp.clicked() {
                    add = Some(i);
                }
                ui.add_space(3.0);
            }
        });
        if let Some(i) = add {
            let m = self.media[i].clone();
            self.tl.add_media(&m);
        }
    }

    fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        panel_title(ui, "EFFECT CONTROLS");
        let Some((ti, ci)) = self.tl.selected else {
            ui.add_space(6.0);
            ui.weak("Tap sebuah clip di timeline");
            ui.weak("untuk mengedit propertinya.");
            return;
        };
        if ci >= self.tl.tracks[ti].clips.len() {
            return;
        }
        let mut c = self.tl.tracks[ti].clips[ci].clone();
        ui.label(RichText::new(&c.name).strong());
        ui.label(RichText::new(c.kind.label()).small().color(c.kind.color()));
        ui.add_space(6.0);

        let mut begun = false;
        let mut changed = false;
        egui::Grid::new("props").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
            ui.label("Mulai");
            let r = ui.add(egui::DragValue::new(&mut c.start).speed(0.05).range(0.0..=36000.0).suffix(" s"));
            begun |= r.drag_started() || r.gained_focus();
            changed |= r.changed();
            ui.end_row();

            ui.label("Durasi");
            let max = c.max_len().min(36000.0);
            let r = ui.add(egui::DragValue::new(&mut c.len).speed(0.05).range(0.1..=max).suffix(" s"));
            begun |= r.drag_started() || r.gained_focus();
            changed |= r.changed();
            ui.end_row();

            ui.label("Sumber");
            ui.label(RichText::new(format!("{:.1} s", c.src_in)).color(TEXT_DIM));
            ui.end_row();
        });
        ui.add_space(6.0);
        if c.kind == Kind::Audio {
            ui.label("Volume");
            let r = ui.add(egui::Slider::new(&mut c.volume, 0.0..=1.0).show_value(true));
            begun |= r.drag_started();
            changed |= r.changed();
        } else {
            ui.label("Opacity");
            let r = ui.add(egui::Slider::new(&mut c.opacity, 0.0..=1.0).show_value(true));
            begun |= r.drag_started();
            changed |= r.changed();
        }
        if begun {
            self.tl.push_undo();
        }
        if changed {
            self.tl.tracks[ti].clips[ci] = c;
        }
    }

    fn monitor(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(6.0)))
            .show(ctx, |ui| {
                let avail = ui.available_size();
                let w = (avail.y * 16.0 / 9.0).min(avail.x);
                let h = w * 9.0 / 16.0;
                let (area, _) = ui.allocate_exact_size(avail, Sense::hover());
                let rect = Rect::from_center_size(area.center(), Vec2::new(w, h));
                let p = ui.painter_at(area);
                p.rect_filled(rect, 4.0, Color32::BLACK);

                let vis = self.visual_at();
                match (&vis, &self.tex) {
                    (Some((path, _, _, opacity)), Some(tex)) if self.tex_path.as_deref() == Some(path.as_str()) => {
                        let [tw, th] = tex.size();
                        let sc = (rect.width() / tw as f32).min(rect.height() / th as f32);
                        let ir = Rect::from_center_size(rect.center(), Vec2::new(tw as f32 * sc, th as f32 * sc));
                        let tint = Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0) as u8);
                        p.image(tex.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
                    }
                    (Some(_), _) => {
                        p.text(rect.center(), Align2::CENTER_CENTER, "Memuat frame...", FontId::proportional(12.0), Color32::from_gray(110));
                    }
                    (None, _) => {
                        let msg = if self.tl.duration() > 0.0 { "" } else { "Import media, lalu tap + untuk menaruhnya di timeline" };
                        p.text(rect.center(), Align2::CENTER_CENTER, msg, FontId::proportional(12.0), Color32::from_gray(100));
                    }
                }
                p.text(
                    rect.left_bottom() + Vec2::new(8.0, -6.0),
                    Align2::LEFT_BOTTOM,
                    format!("{} / {}", Timeline::timecode(self.tl.playhead), Timeline::timecode(self.tl.duration())),
                    FontId::monospace(11.0),
                    Color32::from_white_alpha(170),
                );
                p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(55)));
            });
    }
}

impl eframe::App for ClipForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        if now - self.last_immersive > 1.0 {
            platform::set_immersive();
            self.last_immersive = now;
        }
        ctx.request_repaint_after(Duration::from_millis(1000));

        if !self.status.is_empty() && now > self.status_until {
            self.status.clear();
        }

        // jam playback (audio mengikuti playhead)
        if self.playing {
            let dt = ctx.input(|i| i.stable_dt) as f64;
            self.tl.playhead += dt;
            let dur = self.tl.duration();
            if self.tl.playhead >= dur {
                self.tl.playhead = dur;
                self.playing = false;
            }
            self.tl.follow_playhead();
            ctx.request_repaint();
        }

        if !ctx.wants_keyboard_input() {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                self.toggle_play();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::C)) {
                self.tl.split_at_playhead();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
                self.tl.delete_selected();
            }
        }

        self.top_bar(ctx);

        let tl_h = (ctx.screen_rect().height() * 0.42).max(170.0);
        let mut playing = self.playing;
        egui::TopBottomPanel::bottom("timeline")
            .exact_height(tl_h)
            .resizable(false)
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(5.0)))
            .show(ctx, |ui| self.tl.ui(ui, &mut playing));
        self.playing = playing;

        self.drawer_panel(ctx);
        self.monitor(ctx);
        self.pump_av(ctx);

        if let Some(msg) = self.browser.ui(ctx, &mut self.media) {
            self.say(ctx, msg);
        }
    }
}

fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native("Clip Forge", options, Box::new(|cc| Ok(Box::new(ClipForge::new(cc)))))
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
    platform::init(app.clone());
    let mut options = eframe::NativeOptions::default();
    options.event_loop_builder = Some(Box::new(move |builder| {
        builder.with_android_app(app);
    }));
    run(options).unwrap();
}

#[cfg(not(target_os = "android"))]
pub fn run_desktop() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    run(options)
}
