mod timeline;

use egui::{Color32, FontId, Sense, Stroke, Vec2};
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use timeline::Timeline;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Project,
    Program,
    Effects,
    Timeline,
}

struct Data {
    timeline: Timeline,
    playing: bool,
}

impl TabViewer for Data {
    type Tab = Tab;

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Project => "Project",
            Tab::Program => "Program",
            Tab::Effects => "Effect Controls",
            Tab::Timeline => "Timeline",
        }
        .into()
    }

    fn closeable(&mut self, _tab: &mut Tab) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Project => {
                ui.label("Media (tap untuk tambah ke timeline)");
                ui.separator();
                for n in ["Intro.mp4", "Main.mp4", "B-roll.mp4", "Music.mp3", "Title.png"] {
                    ui.selectable_label(false, format!("🎞 {n}"));
                }
                ui.add_space(8.0);
                if ui.button("＋ Import…").clicked() {
                    // TODO: Android Storage Access Framework (file picker) via JNI
                }
            }
            Tab::Program => self.program_ui(ui),
            Tab::Effects => {
                match self.timeline.selected {
                    Some((t, c)) => {
                        let clip = &mut self.timeline.tracks[t].clips[c];
                        ui.heading(&clip.name);
                        ui.separator();
                        ui.label("Motion");
                        ui.add(egui::Slider::new(&mut clip.len, 0.5..=60.0).text("Duration (s)"));
                        ui.add(egui::Slider::new(&mut clip.start, 0.0..=120.0).text("Start (s)"));
                        ui.collapsing("Opacity", |ui| {
                            ui.label("TODO: keyframes");
                        });
                        ui.collapsing("Lumetri Color", |ui| {
                            ui.label("TODO: wgpu color grading");
                        });
                    }
                    None => {
                        ui.weak("Pilih clip di timeline");
                    }
                }
            }
            Tab::Timeline => self.timeline.ui(ui),
        }
    }
}

impl Data {
    fn program_ui(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let controls_h = 36.0;
        let h = (avail.y - controls_h).max(40.0);
        let w = (h * 16.0 / 9.0).min(avail.x);
        let h = w * 9.0 / 16.0;

        ui.vertical_centered(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, Color32::BLACK);
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(70)));
            // TODO: tampilkan frame hasil decode (MediaCodec) + komposit wgpu
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Program Monitor",
                FontId::proportional(14.0),
                Color32::from_gray(110),
            );

            ui.horizontal(|ui| {
                ui.monospace(Timeline::timecode(self.timeline.playhead));
                if ui.button("⏮").clicked() {
                    self.timeline.playhead = 0.0;
                }
                if ui.button(if self.playing { "⏸" } else { "▶" }).clicked() {
                    self.playing = !self.playing;
                }
                if ui.button("⏭").clicked() {
                    self.timeline.playhead = self.timeline.duration();
                }
                ui.monospace(Timeline::timecode(self.timeline.duration()));
            });
        });
    }
}

pub struct ClipForge {
    dock: DockState<Tab>,
    data: Data,
}

impl ClipForge {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        // Target sentuh lebih besar di HP
        cc.egui_ctx.style_mut(|s| {
            s.spacing.interact_size.y = 30.0;
            s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        });

        // Layout ala Premiere: [Project | Program | Effects] di atas, Timeline di bawah
        let mut dock = DockState::new(vec![Tab::Program]);
        let surface = dock.main_surface_mut();
        let [top, _bottom] = surface.split_below(NodeIndex::root(), 0.55, vec![Tab::Timeline]);
        surface.split_right(top, 0.72, vec![Tab::Effects]);
        surface.split_left(top, 0.25, vec![Tab::Project]);

        Self {
            dock,
            data: Data { timeline: Timeline::demo(), playing: false },
        }
    }
}

impl eframe::App for ClipForge {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Playback clock (placeholder, nanti disinkron ke audio clock)
        if self.data.playing {
            let dt = ctx.input(|i| i.stable_dt) as f64;
            self.data.timeline.playhead += dt;
            if self.data.timeline.playhead >= self.data.timeline.duration() {
                self.data.timeline.playhead = self.data.timeline.duration();
                self.data.playing = false;
            }
            ctx.request_repaint();
        }

        // Shortcut ala Premiere
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.data.playing = !self.data.playing;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.data.timeline.split_at_playhead();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.data.timeline.delete_selected();
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.strong("Clip Forge");
                ui.menu_button("File", |ui| {
                    let _ = ui.button("New Project");
                    let _ = ui.button("Import…");
                    let _ = ui.button("Export (MP4)…");
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Split at playhead").clicked() {
                        self.data.timeline.split_at_playhead();
                        ui.close_menu();
                    }
                    if ui.button("Delete clip").clicked() {
                        self.data.timeline.delete_selected();
                        ui.close_menu();
                    }
                });
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                DockArea::new(&mut self.dock)
                    .style(Style::from_egui(ui.style().as_ref()))
                    .show_inside(ui, &mut self.data);
            });
    }
}

fn run(options: eframe::NativeOptions) -> eframe::Result {
    eframe::run_native(
        "Clip Forge",
        options,
        Box::new(|cc| Ok(Box::new(ClipForge::new(cc)))),
    )
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info),
    );
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
