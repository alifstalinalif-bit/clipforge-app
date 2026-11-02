mod browser;
mod platform;
mod theme;
mod timeline;

use browser::Browser;
use egui::{Align2, Color32, FontId, Frame, Margin, Pos2, Rect, RichText, ScrollArea, Sense, Vec2};
use std::time::Duration;
use theme::*;
use timeline::{Kind, MediaItem, Timeline, FPS};

pub struct ClipForge {
    tl: Timeline,
    media: Vec<MediaItem>,
    browser: Browser,
    playing: bool,
    status: String,
    status_until: f64,
    last_immersive: f64,
}

impl ClipForge {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        platform::set_immersive();
        Self {
            tl: Timeline::new(),
            media: vec![],
            browser: Browser::new(),
            playing: false,
            status: String::new(),
            status_until: 0.0,
            last_immersive: 0.0,
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

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top")
            .exact_height(44.0)
            .frame(Frame::none().fill(HEADER).inner_margin(Margin::symmetric(10.0, 5.0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("CLIP FORGE").strong().size(16.0).color(ACCENT));
                    ui.separator();
                    if primary_button(ui, "+ Import").clicked() {
                        self.browser.open();
                    }
                    ui.add_enabled(false, egui::Button::new("Export"))
                        .on_disabled_hover_text("Segera hadir");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.monospace(Timeline::timecode(self.tl.duration()));
                        ui.label(RichText::new("Durasi").small().color(TEXT_DIM));
                        if !self.status.is_empty() {
                            ui.separator();
                            ui.label(RichText::new(&self.status).color(Color32::from_rgb(255, 210, 90)));
                        }
                    });
                });
            });
    }

    fn project_panel(&mut self, ctx: &egui::Context) {
        let w = (ctx.screen_rect().width() * 0.23).clamp(170.0, 270.0);
        let mut add: Option<usize> = None;
        egui::SidePanel::left("project")
            .exact_width(w)
            .resizable(false)
            .frame(Frame::none().fill(PANEL).inner_margin(Margin::same(8.0)))
            .show(ctx, |ui| {
                panel_title(ui, "PROJECT");
                if self.media.is_empty() {
                    ui.add_space(6.0);
                    ui.weak("Belum ada media.");
                    ui.weak("Tap + Import di atas.");
                }
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, m) in self.media.iter().enumerate() {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::hover());
                        let add_rect = Rect::from_min_size(
                            Pos2::new(rect.right() - 42.0, rect.top()),
                            Vec2::new(42.0, 42.0),
                        );
                        let add_resp = ui.interact(add_rect, ui.id().with(("add", i)), Sense::click());
                        let p = ui.painter_at(rect);
                        p.rect_filled(rect, 6.0, Color32::from_gray(44));
                        p.rect_filled(
                            Rect::from_min_size(rect.left_top(), Vec2::new(5.0, rect.height())),
                            2.0,
                            m.kind.color(),
                        );
                        let text_clip = Rect::from_min_max(rect.min, Pos2::new(add_rect.left() - 2.0, rect.max.y));
                        let tp = p.with_clip_rect(text_clip);
                        tp.text(
                            Pos2::new(rect.left() + 14.0, rect.top() + 14.0),
                            Align2::LEFT_CENTER,
                            &m.name,
                            FontId::proportional(13.0),
                            Color32::WHITE,
                        );
                        let sub = if m.kind == Kind::Image {
                            "Gambar".to_string()
                        } else {
                            format!("{}  {}", m.kind.label(), Self::fmt_dur(m.duration))
                        };
                        tp.text(
                            Pos2::new(rect.left() + 14.0, rect.top() + 30.0),
                            Align2::LEFT_CENTER,
                            sub,
                            FontId::proportional(10.5),
                            TEXT_DIM,
                        );
                        let br = add_rect.shrink2(Vec2::new(5.0, 6.0));
                        let col = if add_resp.is_pointer_button_down_on() { Color32::from_rgb(30, 100, 180) } else { ACCENT };
                        p.rect_filled(br, 6.0, col);
                        p.text(br.center(), Align2::CENTER_CENTER, "+", FontId::proportional(20.0), Color32::WHITE);
                        if add_resp.clicked() {
                            add = Some(i);
                        }
                        ui.add_space(4.0);
                    }
                });
            });
        if let Some(i) = add {
            let m = self.media[i].clone();
            self.tl.add_media(&m);
        }
    }

    fn inspector(&mut self, ctx: &egui::Context) {
        let w = (ctx.screen_rect().width() * 0.24).clamp(180.0, 290.0);
        egui::SidePanel::right("inspector")
            .exact_width(w)
            .resizable(false)
            .frame(Frame::none().fill(PANEL).inner_margin(Margin::same(10.0)))
            .show(ctx, |ui| {
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
                    let r = ui.add(egui::Slider::new(&mut c.volume, 0.0..=2.0).show_value(true));
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
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Opacity/volume tersimpan di clip; efek, transisi, dan keyframe dirender di tahap berikutnya.")
                        .small()
                        .color(TEXT_DIM),
                );
            });
    }

    fn monitor(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(8.0)))
            .show(ctx, |ui| {
                let avail = ui.available_size();
                let ctrl_h = 42.0;
                let area_h = (avail.y - ctrl_h - 8.0).max(40.0);
                let w = (area_h * 16.0 / 9.0).min(avail.x);
                let h = w * 9.0 / 16.0;

                ui.vertical_centered(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 4.0, Color32::BLACK);
                    p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(60)));

                    // clip video teratas di posisi playhead
                    let t = self.tl.playhead;
                    let top = self
                        .tl
                        .tracks
                        .iter()
                        .filter(|tr| !tr.audio)
                        .flat_map(|tr| tr.clips.iter())
                        .find(|c| t >= c.start && t < c.end());
                    match top {
                        Some(c) => {
                            p.text(rect.center() - Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, &c.name, FontId::proportional(16.0), Color32::from_gray(200));
                            p.text(rect.center() + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "Preview frame video menyusul (tahap decoder)", FontId::proportional(11.0), Color32::from_gray(110));
                        }
                        None => {
                            p.text(rect.center(), Align2::CENTER_CENTER, "Program Monitor", FontId::proportional(14.0), Color32::from_gray(90));
                        }
                    }

                    ui.add_space(6.0);
                    let (row, _) = ui.allocate_exact_size(Vec2::new(avail.x, ctrl_h), Sense::hover());
                    let bw = 46.0;
                    let gap = 8.0;
                    let total = 5.0 * bw + 4.0 * gap;
                    let x0 = row.center().x - total / 2.0;
                    let slot = |i: usize| {
                        Rect::from_min_size(Pos2::new(x0 + i as f32 * (bw + gap), row.top() + 3.0), Vec2::new(bw, 36.0))
                    };
                    let dur = self.tl.duration();
                    if icon_button(ui, slot(0), "tostart", Icon::ToStart, false).clicked() {
                        self.tl.playhead = 0.0;
                        self.tl.scroll = 0.0;
                    }
                    if icon_button(ui, slot(1), "back", Icon::StepBack, false).clicked() {
                        self.tl.playhead = (self.tl.playhead - 1.0 / FPS).max(0.0);
                    }
                    let pi = if self.playing { Icon::Pause } else { Icon::Play };
                    if icon_button(ui, slot(2), "play", pi, true).clicked() {
                        self.toggle_play();
                    }
                    if icon_button(ui, slot(3), "fwd", Icon::StepFwd, false).clicked() {
                        self.tl.playhead = (self.tl.playhead + 1.0 / FPS).min(dur);
                    }
                    if icon_button(ui, slot(4), "toend", Icon::ToEnd, false).clicked() {
                        self.tl.playhead = dur;
                    }
                    let rp = ui.painter_at(row);
                    rp.text(
                        Pos2::new(row.left() + 4.0, row.center().y),
                        Align2::LEFT_CENTER,
                        format!("{} / {}", Timeline::timecode(self.tl.playhead), Timeline::timecode(dur)),
                        FontId::monospace(12.0),
                        Color32::from_gray(200),
                    );
                });
            });
    }

    fn toggle_play(&mut self) {
        if self.tl.duration() <= 0.0 {
            return;
        }
        if !self.playing && self.tl.playhead >= self.tl.duration() - 0.01 {
            self.tl.playhead = 0.0;
        }
        self.playing = !self.playing;
    }
}

impl eframe::App for ClipForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        // Sembunyikan status bar/navigasi (Android) dan pastikan tetap tersembunyi
        if now - self.last_immersive > 1.0 {
            platform::set_immersive();
            self.last_immersive = now;
        }
        ctx.request_repaint_after(Duration::from_millis(1000));

        if !self.status.is_empty() && now > self.status_until {
            self.status.clear();
        }

        // Playback (jam sementara; nanti disinkron ke audio)
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

        // Shortcut keyboard (kalau ada keyboard eksternal)
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

        let tl_h = (ctx.screen_rect().height() * 0.47).max(180.0);
        egui::TopBottomPanel::bottom("timeline")
            .exact_height(tl_h)
            .resizable(false)
            .frame(Frame::none().fill(BG).inner_margin(Margin::same(6.0)))
            .show(ctx, |ui| self.tl.ui(ui));

        self.project_panel(ctx);
        self.inspector(ctx);
        self.monitor(ctx);

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
