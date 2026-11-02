use crate::platform;
use crate::theme::*;
use crate::timeline::{Kind, MediaItem};
use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense, Vec2};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Entry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    kind: Option<Kind>,
}

pub struct Browser {
    pub open: bool,
    root: PathBuf,
    dir: PathBuf,
    entries: Vec<Entry>,
    error: Option<String>,
    perm_ok: bool,
    last_check: f64,
}

pub fn kind_of(path: &Path) -> Option<Kind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "mp4" | "mov" | "mkv" | "webm" | "3gp" | "avi" | "m4v" => Some(Kind::Video),
        "mp3" | "m4a" | "aac" | "wav" | "ogg" | "flac" | "opus" => Some(Kind::Audio),
        "jpg" | "jpeg" | "png" | "webp" => Some(Kind::Image),
        _ => None,
    }
}

fn default_root() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        let p = PathBuf::from("/storage/emulated/0");
        if p.exists() {
            return p;
        }
        return PathBuf::from("/sdcard");
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"))
    }
}

impl Browser {
    pub fn new() -> Self {
        let root = default_root();
        Self {
            open: false,
            dir: root.clone(),
            root,
            entries: vec![],
            error: None,
            perm_ok: true,
            last_check: 0.0,
        }
    }

    pub fn open(&mut self) {
        self.open = true;
        self.perm_ok = platform::has_media_permission();
        self.refresh();
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        continue;
                    }
                    let path = e.path();
                    let is_dir = path.is_dir();
                    let kind = if is_dir { None } else { kind_of(&path) };
                    if is_dir || kind.is_some() {
                        self.entries.push(Entry { name, path, is_dir, kind });
                    }
                }
                self.entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
            }
            Err(e) => self.error = Some(format!("Tidak bisa membuka folder: {e}")),
        }
    }

    fn goto(&mut self, p: PathBuf) {
        self.dir = p;
        self.refresh();
    }

    /// Menggambar jendela import. Mengembalikan pesan status bila ada file yang masuk.
    pub fn ui(&mut self, ctx: &egui::Context, media: &mut Vec<MediaItem>) -> Option<String> {
        if !self.open {
            return None;
        }

        if !self.perm_ok {
            let now = ctx.input(|i| i.time);
            if now - self.last_check > 0.5 {
                self.last_check = now;
                if platform::has_media_permission() {
                    self.perm_ok = true;
                    self.refresh();
                }
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }

        let sr = ctx.screen_rect();
        let size = Vec2::new((sr.width() * 0.72).min(720.0), sr.height() * 0.82);
        let mut win_open = true;
        let mut close = false;
        let mut goto: Option<PathBuf> = None;
        let mut picks: Vec<usize> = vec![];
        let mut ask_perm = false;

        egui::Window::new("Import media")
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(size)
            .collapsible(false)
            .resizable(false)
            .open(&mut win_open)
            .show(ctx, |ui| {
                if !self.perm_ok {
                    ui.add_space(20.0);
                    ui.label("Clip Forge perlu izin untuk membaca video, audio, dan foto di HP.");
                    ui.add_space(10.0);
                    if primary_button(ui, "Izinkan akses media").clicked() {
                        ask_perm = true;
                    }
                    ui.add_space(8.0);
                    ui.weak("Kalau dialog izin tidak muncul: Pengaturan > Aplikasi > Clip Forge > Izin > Media.");
                    return;
                }

                ui.horizontal_wrapped(|ui| {
                    let can_up = self.dir != self.root && self.dir.parent().is_some();
                    if ui.add_enabled(can_up, egui::Button::new("< Naik")).clicked() {
                        goto = self.dir.parent().map(|p| p.to_path_buf());
                    }
                    for name in ["DCIM", "Download", "Movies", "Music", "Pictures"] {
                        if ui.button(name).clicked() {
                            goto = Some(self.root.join(name));
                        }
                    }
                });
                ui.label(
                    RichText::new(self.dir.to_string_lossy().to_string())
                        .small()
                        .color(TEXT_DIM),
                );
                ui.separator();

                if let Some(err) = &self.error {
                    ui.colored_label(Color32::from_rgb(235, 110, 110), err);
                } else if self.entries.is_empty() {
                    ui.weak("Folder ini tidak berisi video, audio, atau gambar.");
                }

                let list_h = (ui.available_height() - 46.0).max(60.0);
                ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                    for (idx, e) in self.entries.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 40.0),
                            Sense::click(),
                        );
                        let bg = if resp.is_pointer_button_down_on() {
                            Color32::from_gray(80)
                        } else if idx % 2 == 0 {
                            Color32::from_gray(40)
                        } else {
                            Color32::from_gray(35)
                        };
                        let p = ui.painter();
                        p.rect_filled(rect, 4.0, bg);
                        let tag = match (e.is_dir, e.kind) {
                            (true, _) => Color32::from_rgb(205, 170, 80),
                            (_, Some(k)) => k.color(),
                            _ => Color32::GRAY,
                        };
                        p.rect_filled(
                            Rect::from_center_size(
                                Pos2::new(rect.left() + 18.0, rect.center().y),
                                Vec2::new(14.0, 14.0),
                            ),
                            3.0,
                            tag,
                        );
                        p.text(
                            Pos2::new(rect.left() + 36.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            &e.name,
                            FontId::proportional(14.0),
                            Color32::WHITE,
                        );
                        let already = media.iter().any(|m| m.path == e.path.to_string_lossy());
                        let right = if e.is_dir {
                            ">".to_string()
                        } else if already {
                            "sudah diimpor".to_string()
                        } else {
                            e.kind.map(|k| k.label().to_string()).unwrap_or_default()
                        };
                        p.text(
                            Pos2::new(rect.right() - 10.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            right,
                            FontId::proportional(11.0),
                            TEXT_DIM,
                        );
                        if resp.clicked() {
                            if e.is_dir {
                                goto = Some(e.path.clone());
                            } else if !already {
                                picks.push(idx);
                            }
                        }
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.weak("Tap file untuk mengimpor.");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary_button(ui, "Selesai").clicked() {
                            close = true;
                        }
                    });
                });
            });

        if ask_perm {
            platform::request_media_permissions();
        }
        if let Some(p) = goto {
            self.goto(p);
        }

        let mut status = None;
        if !picks.is_empty() {
            let mut added = 0;
            let mut failed = 0;
            for idx in picks {
                let e = &self.entries[idx];
                let Some(kind) = e.kind else { continue };
                let path = e.path.to_string_lossy().to_string();
                let duration = if kind == Kind::Image {
                    Some(5.0)
                } else {
                    platform::media_duration(&path)
                };
                match duration {
                    Some(d) if d > 0.0 => {
                        media.push(MediaItem { name: e.name.clone(), path, kind, duration: d });
                        added += 1;
                    }
                    _ => failed += 1,
                }
            }
            status = Some(match (added, failed) {
                (a, 0) => format!("{a} file masuk ke Project"),
                (0, f) => format!("Gagal membaca {f} file"),
                (a, f) => format!("{a} file masuk, {f} gagal dibaca"),
            });
        }

        if close || !win_open {
            self.open = false;
        }
        status
    }
}
