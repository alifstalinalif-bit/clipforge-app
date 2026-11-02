use crate::theme::*;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2};

pub const FPS: f64 = 30.0;
const HEADER_W: f32 = 52.0;
const RULER_H: f32 = 24.0;
const HANDLE_W: f32 = 24.0;
const MIN_LEN: f64 = 0.1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Audio,
    Image,
}

impl Kind {
    pub fn color(self) -> Color32 {
        match self {
            Kind::Video => VIDEO,
            Kind::Audio => AUDIO,
            Kind::Image => IMAGE,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => "Video",
            Kind::Audio => "Audio",
            Kind::Image => "Gambar",
        }
    }
}

#[derive(Clone)]
pub struct MediaItem {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub duration: f64,
}

#[derive(Clone)]
pub struct Clip {
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub start: f64,   // posisi di timeline (detik)
    pub len: f64,     // durasi di timeline
    pub src_in: f64,  // titik mulai di file sumber
    pub src_dur: f64, // durasi total file sumber (INFINITY untuk gambar)
    pub opacity: f32,
    pub volume: f32,
}

impl Clip {
    pub fn end(&self) -> f64 {
        self.start + self.len
    }
    pub fn max_len(&self) -> f64 {
        self.src_dur - self.src_in
    }
}

#[derive(Clone)]
pub struct Track {
    pub name: String,
    pub audio: bool,
    pub clips: Vec<Clip>,
}

impl Track {
    fn accepts(&self, k: Kind) -> bool {
        self.audio == (k == Kind::Audio)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Scrub,
    Pan,
    Move { ti: usize, ci: usize, grab: f64 },
    TrimL { ti: usize, ci: usize },
    TrimR { ti: usize, ci: usize },
}

#[derive(Clone, Copy)]
struct Geo {
    rect: Rect,
    zoom: f32,
    scroll: f64,
    track_h: f32,
}

impl Geo {
    fn x(&self, t: f64) -> f32 {
        self.rect.left() + HEADER_W + ((t - self.scroll) as f32) * self.zoom
    }
    fn t(&self, x: f32) -> f64 {
        ((x - self.rect.left() - HEADER_W) / self.zoom) as f64 + self.scroll
    }
    fn track_top(&self, i: usize) -> f32 {
        self.rect.top() + RULER_H + i as f32 * self.track_h
    }
    fn track_at(&self, y: f32, n: usize) -> Option<usize> {
        let r = (y - self.rect.top() - RULER_H) / self.track_h;
        if r < 0.0 {
            return None;
        }
        let i = r as usize;
        (i < n).then_some(i)
    }
    fn clip_rect(&self, ti: usize, c: &Clip) -> Rect {
        let top = self.track_top(ti);
        Rect::from_min_max(
            Pos2::new(self.x(c.start), top + 2.0),
            Pos2::new(self.x(c.end()), top + self.track_h - 2.0),
        )
    }
}

pub struct Timeline {
    pub tracks: Vec<Track>,
    pub playhead: f64,
    pub zoom: f32, // piksel per detik
    pub scroll: f64,
    pub selected: Option<(usize, usize)>,
    pub snap: bool,
    snap_line: Option<f64>,
    drag: Drag,
    undo: Vec<Vec<Track>>,
    redo: Vec<Vec<Track>>,
    last_width: f32,
}

impl Timeline {
    pub fn new() -> Self {
        let t = |n: &str, audio: bool| Track { name: n.into(), audio, clips: vec![] };
        Self {
            tracks: vec![t("V2", false), t("V1", false), t("A1", true), t("A2", true)],
            playhead: 0.0,
            zoom: 60.0,
            scroll: 0.0,
            selected: None,
            snap: true,
            snap_line: None,
            drag: Drag::None,
            undo: vec![],
            redo: vec![],
            last_width: 800.0,
        }
    }

    pub fn duration(&self) -> f64 {
        self.tracks
            .iter()
            .flat_map(|t| t.clips.iter())
            .map(|c| c.end())
            .fold(0.0, f64::max)
    }

    pub fn timecode(t: f64) -> String {
        let fps = FPS as u64;
        let total = (t.max(0.0) * FPS).round() as u64;
        let f = total % fps;
        let s = (total / fps) % 60;
        let m = (total / fps / 60) % 60;
        let h = total / fps / 3600;
        format!("{h:02}:{m:02}:{s:02}:{f:02}")
    }

    // ---------- undo / redo ----------

    pub fn push_undo(&mut self) {
        self.undo.push(self.tracks.clone());
        if self.undo.len() > 60 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.tracks, prev));
            self.selected = None;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.tracks, next));
            self.selected = None;
        }
    }

    // ---------- editing ----------

    fn track_end(&self, ti: usize) -> f64 {
        self.tracks[ti].clips.iter().map(|c| c.end()).fold(0.0, f64::max)
    }

    fn make_clip(m: &MediaItem, start: f64) -> Clip {
        Clip {
            name: m.name.clone(),
            path: m.path.clone(),
            kind: m.kind,
            start,
            len: m.duration.max(MIN_LEN),
            src_in: 0.0,
            src_dur: if m.kind == Kind::Image { f64::INFINITY } else { m.duration },
            opacity: 1.0,
            volume: 1.0,
        }
    }

    /// Tambah media di ujung track (video -> V1 + audio di A1).
    pub fn add_media(&mut self, m: &MediaItem) {
        self.push_undo();
        let (v1, a1) = (1usize, 2usize);
        match m.kind {
            Kind::Audio => {
                let s = self.track_end(a1);
                self.tracks[a1].clips.push(Self::make_clip(m, s));
                self.selected = Some((a1, self.tracks[a1].clips.len() - 1));
                self.playhead = s;
            }
            _ => {
                let s = self.track_end(v1);
                self.tracks[v1].clips.push(Self::make_clip(m, s));
                let sel = (v1, self.tracks[v1].clips.len() - 1);
                if m.kind == Kind::Video {
                    let mut a = Self::make_clip(m, s);
                    a.kind = Kind::Audio;
                    self.tracks[a1].clips.push(a);
                }
                self.selected = Some(sel);
                self.playhead = s;
            }
        }
        if self.playhead < self.scroll {
            self.scroll = self.playhead;
        }
    }

    pub fn split_at_playhead(&mut self) -> bool {
        let t = self.playhead;
        let inside = |c: &Clip| t > c.start + MIN_LEN && t < c.end() - MIN_LEN;
        let mut targets: Vec<(usize, usize)> = vec![];
        if let Some((ti, ci)) = self.selected {
            if ci < self.tracks[ti].clips.len() && inside(&self.tracks[ti].clips[ci]) {
                targets.push((ti, ci));
            }
        }
        if targets.is_empty() {
            for (ti, tr) in self.tracks.iter().enumerate() {
                for (ci, c) in tr.clips.iter().enumerate() {
                    if inside(c) {
                        targets.push((ti, ci));
                    }
                }
            }
        }
        if targets.is_empty() {
            return false;
        }
        self.push_undo();
        for (ti, ci) in targets {
            let c = &mut self.tracks[ti].clips[ci];
            let mut right = c.clone();
            right.start = t;
            right.src_in = c.src_in + (t - c.start);
            right.len = c.end() - t;
            c.len = t - c.start;
            self.tracks[ti].clips.push(right);
        }
        self.selected = None;
        true
    }

    pub fn delete_selected(&mut self) {
        if let Some((ti, ci)) = self.selected {
            if ci < self.tracks[ti].clips.len() {
                self.push_undo();
                self.tracks[ti].clips.remove(ci);
            }
            self.selected = None;
        }
    }

    pub fn duplicate_selected(&mut self) {
        if let Some((ti, ci)) = self.selected {
            if ci < self.tracks[ti].clips.len() {
                self.push_undo();
                let mut c = self.tracks[ti].clips[ci].clone();
                c.start = c.end();
                self.tracks[ti].clips.push(c);
                self.selected = Some((ti, self.tracks[ti].clips.len() - 1));
            }
        }
    }

    pub fn follow_playhead(&mut self) {
        let vis = ((self.last_width - HEADER_W) / self.zoom) as f64;
        if self.playhead > self.scroll + vis * 0.95 || self.playhead < self.scroll {
            self.scroll = (self.playhead - vis * 0.1).max(0.0);
        }
    }

    fn fit(&mut self) {
        let d = self.duration().max(1.0);
        self.zoom = (((self.last_width - HEADER_W - 24.0) as f64 / d) as f32).clamp(6.0, 600.0);
        self.scroll = 0.0;
    }

    // ---------- snapping ----------

    fn snap_points(&self, ignore: (usize, usize)) -> Vec<f64> {
        let mut pts = vec![0.0, self.playhead];
        for (ti, tr) in self.tracks.iter().enumerate() {
            for (ci, c) in tr.clips.iter().enumerate() {
                if (ti, ci) != ignore {
                    pts.push(c.start);
                    pts.push(c.end());
                }
            }
        }
        pts
    }

    fn snap_time(&mut self, t: f64, ignore: (usize, usize)) -> f64 {
        self.snap_line = None;
        if !self.snap {
            return t;
        }
        let thr = 10.0 / self.zoom as f64;
        let mut best: Option<(f64, f64)> = None;
        for p in self.snap_points(ignore) {
            let d = (t - p).abs();
            if d < thr && best.map_or(true, |b| d < b.0) {
                best = Some((d, p));
            }
        }
        match best {
            Some((_, p)) => {
                self.snap_line = Some(p);
                p
            }
            None => t,
        }
    }

    fn snap_edges(&mut self, start: f64, len: f64, ignore: (usize, usize)) -> f64 {
        self.snap_line = None;
        if !self.snap {
            return start;
        }
        let thr = 10.0 / self.zoom as f64;
        let mut best: Option<(f64, f64, f64)> = None; // (jarak, start baru, garis)
        for p in self.snap_points(ignore) {
            for (edge, off) in [(start, 0.0), (start + len, len)] {
                let d = (edge - p).abs();
                if d < thr && best.map_or(true, |b| d < b.0) {
                    best = Some((d, p - off, p));
                }
            }
        }
        match best {
            Some((_, s, line)) => {
                self.snap_line = Some(line);
                s.max(0.0)
            }
            None => start,
        }
    }

    // ---------- interaksi ----------

    fn hit_test(&self, g: &Geo, p: Pos2) -> Drag {
        if p.y < g.rect.top() + RULER_H {
            return if p.x >= g.rect.left() + HEADER_W { Drag::Scrub } else { Drag::None };
        }
        if p.x < g.rect.left() + HEADER_W {
            return Drag::None;
        }
        if (p.x - g.x(self.playhead)).abs() < 10.0 {
            return Drag::Scrub;
        }
        if let Some(ti) = g.track_at(p.y, self.tracks.len()) {
            for ci in (0..self.tracks[ti].clips.len()).rev() {
                let c = &self.tracks[ti].clips[ci];
                let r = g.clip_rect(ti, c);
                let sel = self.selected == Some((ti, ci));
                let hit = if sel { r.expand2(Vec2::new(10.0, 0.0)) } else { r };
                if p.x >= hit.left() && p.x <= hit.right() {
                    let hw = HANDLE_W.min(r.width() * 0.35);
                    if sel && p.x - r.left() < hw {
                        return Drag::TrimL { ti, ci };
                    }
                    if sel && r.right() - p.x < hw {
                        return Drag::TrimR { ti, ci };
                    }
                    return Drag::Move { ti, ci, grab: g.t(p.x) - c.start };
                }
            }
        }
        Drag::Pan
    }

    fn apply_drag(&mut self, g: &Geo, p: Pos2) {
        let n = self.tracks.len();
        match self.drag {
            Drag::Scrub => self.playhead = g.t(p.x).max(0.0),
            Drag::Move { ti, ci, grab } => {
                if ci >= self.tracks[ti].clips.len() {
                    return;
                }
                let (len, kind) = {
                    let c = &self.tracks[ti].clips[ci];
                    (c.len, c.kind)
                };
                let raw = (g.t(p.x) - grab).max(0.0);
                let start = self.snap_edges(raw, len, (ti, ci));
                let mut nti = ti;
                if let Some(tj) = g.track_at(p.y, n) {
                    if tj != ti && self.tracks[tj].accepts(kind) {
                        nti = tj;
                    }
                }
                if nti == ti {
                    self.tracks[ti].clips[ci].start = start;
                } else {
                    let mut c = self.tracks[ti].clips.remove(ci);
                    c.start = start;
                    self.tracks[nti].clips.push(c);
                    let nci = self.tracks[nti].clips.len() - 1;
                    self.drag = Drag::Move { ti: nti, ci: nci, grab };
                    self.selected = Some((nti, nci));
                }
            }
            Drag::TrimL { ti, ci } => {
                if ci >= self.tracks[ti].clips.len() {
                    return;
                }
                let (s, e, si) = {
                    let c = &self.tracks[ti].clips[ci];
                    (c.start, c.end(), c.src_in)
                };
                let lo = (s - si).max(0.0);
                let hi = e - MIN_LEN;
                let raw = self.snap_time(g.t(p.x), (ti, ci));
                let ns = raw.min(hi).max(lo);
                let c = &mut self.tracks[ti].clips[ci];
                let d = ns - c.start;
                c.src_in += d;
                c.len -= d;
                c.start = ns;
            }
            Drag::TrimR { ti, ci } => {
                if ci >= self.tracks[ti].clips.len() {
                    return;
                }
                let (s, maxl) = {
                    let c = &self.tracks[ti].clips[ci];
                    (c.start, c.max_len())
                };
                let lo = s + MIN_LEN;
                let hi = s + maxl;
                let raw = self.snap_time(g.t(p.x), (ti, ci));
                let ne = raw.min(hi).max(lo);
                self.tracks[ti].clips[ci].len = ne - s;
            }
            Drag::Pan | Drag::None => {}
        }
    }

    fn tap(&mut self, g: &Geo, p: Pos2) {
        match self.hit_test(g, p) {
            Drag::Scrub => self.playhead = g.t(p.x).max(0.0),
            Drag::Move { ti, ci, .. } | Drag::TrimL { ti, ci } | Drag::TrimR { ti, ci } => {
                self.selected = Some((ti, ci))
            }
            Drag::Pan => self.selected = None,
            Drag::None => {}
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let has_sel = self.selected.is_some();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                self.undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                self.redo();
            }
            ui.separator();
            if ui.button("Split").clicked() {
                self.split_at_playhead();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Duplikat")).clicked() {
                self.duplicate_selected();
            }
            if ui.add_enabled(has_sel, egui::Button::new("Hapus")).clicked() {
                self.delete_selected();
            }
            ui.separator();
            ui.toggle_value(&mut self.snap, "Snap");
            ui.separator();
            if ui.button("-").clicked() {
                self.zoom = (self.zoom / 1.4).clamp(6.0, 600.0);
            }
            if ui.button("+").clicked() {
                self.zoom = (self.zoom * 1.4).clamp(6.0, 600.0);
            }
            if ui.button("Fit").clicked() {
                self.fit();
            }
        });
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.toolbar(ui);

        let size = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        self.last_width = rect.width();
        let n = self.tracks.len();
        let track_h = ((rect.height() - RULER_H) / n as f32).clamp(24.0, 48.0);

        // Pinch-to-zoom (dua jari)
        let (zd, mt, hover) = ui.input(|i| (i.zoom_delta(), i.multi_touch(), i.pointer.hover_pos()));
        let pinching = mt.is_some();
        if pinching {
            self.drag = Drag::None;
        }
        if (zd - 1.0).abs() > 1e-4 && (pinching || hover.map_or(false, |h| rect.contains(h))) {
            let cx = mt
                .map(|m| m.start_pos.x)
                .or(hover.map(|h| h.x))
                .unwrap_or(rect.center().x);
            let g0 = Geo { rect, zoom: self.zoom, scroll: self.scroll, track_h };
            let anchor = g0.t(cx);
            self.zoom = (self.zoom * zd).clamp(6.0, 600.0);
            self.scroll = (anchor - ((cx - rect.left() - HEADER_W) / self.zoom) as f64).max(0.0);
        }

        let g = Geo { rect, zoom: self.zoom, scroll: self.scroll, track_h };

        if !pinching {
            if resp.drag_started() {
                if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                    self.drag = self.hit_test(&g, p);
                    match self.drag {
                        Drag::Move { ti, ci, .. } | Drag::TrimL { ti, ci } | Drag::TrimR { ti, ci } => {
                            self.selected = Some((ti, ci));
                            self.push_undo();
                        }
                        Drag::Scrub => self.playhead = g.t(p.x).max(0.0),
                        _ => {}
                    }
                }
            }
            if resp.dragged() {
                if self.drag == Drag::Pan {
                    self.scroll = (self.scroll - resp.drag_delta().x as f64 / self.zoom as f64).max(0.0);
                } else if let Some(p) = resp.interact_pointer_pos() {
                    self.apply_drag(&g, p);
                }
            }
            if resp.drag_stopped() {
                self.drag = Drag::None;
                self.snap_line = None;
            }
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.tap(&g, p);
                }
            }
        }

        // Geometri bisa berubah karena drag (scroll), hitung ulang untuk menggambar
        let g = Geo { rect, zoom: self.zoom, scroll: self.scroll, track_h };
        self.paint(ui, &g);
    }

    fn paint(&self, ui: &Ui, g: &Geo) {
        let rect = g.rect;
        let n = self.tracks.len();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, BG);

        let body = Rect::from_min_max(Pos2::new(rect.left() + HEADER_W, rect.top()), rect.max);
        let bp = painter.with_clip_rect(body);

        // latar track
        for (i, tr) in self.tracks.iter().enumerate() {
            let y = g.track_top(i);
            let col = if tr.audio { Color32::from_rgb(29, 34, 31) } else { Color32::from_rgb(29, 31, 37) };
            bp.rect_filled(
                Rect::from_min_size(Pos2::new(body.left(), y), Vec2::new(body.width(), g.track_h)),
                0.0,
                col,
            );
            bp.line_segment(
                [Pos2::new(body.left(), y + g.track_h), Pos2::new(body.right(), y + g.track_h)],
                Stroke::new(1.0, LINE),
            );
        }

        // ruler
        bp.rect_filled(Rect::from_min_size(body.min, Vec2::new(body.width(), RULER_H)), 0.0, HEADER);
        let steps = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0];
        let step = steps.iter().copied().find(|s| (*s as f32) * self.zoom >= 90.0).unwrap_or(1800.0);
        let mut t = (g.t(body.left()) / step).floor().max(0.0) * step;
        while g.x(t) < body.right() {
            let x = g.x(t);
            bp.line_segment(
                [Pos2::new(x, rect.top() + 13.0), Pos2::new(x, rect.top() + RULER_H)],
                Stroke::new(1.0, TEXT_DIM),
            );
            bp.text(
                Pos2::new(x + 3.0, rect.top() + 1.0),
                Align2::LEFT_TOP,
                Self::timecode(t),
                FontId::monospace(9.0),
                Color32::from_gray(185),
            );
            t += step;
        }

        // clip
        for (ti, tr) in self.tracks.iter().enumerate() {
            for (ci, c) in tr.clips.iter().enumerate() {
                let r = g.clip_rect(ti, c);
                if r.right() < body.left() || r.left() > body.right() {
                    continue;
                }
                let sel = self.selected == Some((ti, ci));
                bp.rect_filled(r, 5.0, c.kind.color());
                bp.rect_stroke(
                    r,
                    5.0,
                    Stroke::new(
                        if sel { 2.0 } else { 1.0 },
                        if sel { Color32::WHITE } else { Color32::from_black_alpha(170) },
                    ),
                );
                let tp = bp.with_clip_rect(r.intersect(body));
                tp.text(
                    Pos2::new(r.left().max(body.left()) + 10.0, r.center().y),
                    Align2::LEFT_CENTER,
                    &c.name,
                    FontId::proportional(12.0),
                    Color32::WHITE,
                );
                if sel {
                    let hw = 8.0_f32.min(r.width() * 0.3);
                    for hr in [
                        Rect::from_min_size(r.left_top(), Vec2::new(hw, r.height())),
                        Rect::from_min_size(Pos2::new(r.right() - hw, r.top()), Vec2::new(hw, r.height())),
                    ] {
                        bp.rect_filled(hr, 3.0, Color32::from_white_alpha(235));
                    }
                }
            }
        }

        if self.tracks.iter().all(|t| t.clips.is_empty()) {
            bp.text(
                Pos2::new(body.center().x, body.top() + RULER_H + (body.height() - RULER_H) / 2.0),
                Align2::CENTER_CENTER,
                "Timeline kosong. Tap + Import, lalu tap + di media.",
                FontId::proportional(13.0),
                TEXT_DIM,
            );
        }

        // garis snap
        if let Some(s) = self.snap_line {
            let x = g.x(s);
            bp.line_segment(
                [Pos2::new(x, rect.top() + RULER_H), Pos2::new(x, rect.bottom())],
                Stroke::new(1.0, Color32::from_rgb(255, 210, 60)),
            );
        }

        // playhead
        let px = g.x(self.playhead);
        bp.line_segment(
            [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
            Stroke::new(2.0, PLAYHEAD),
        );
        bp.add(Shape::convex_polygon(
            vec![
                Pos2::new(px - 8.0, rect.top()),
                Pos2::new(px + 8.0, rect.top()),
                Pos2::new(px, rect.top() + 15.0),
            ],
            PLAYHEAD,
            Stroke::NONE,
        ));

        // kolom header track (di atas konten yang tergeser)
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(HEADER_W, rect.height())),
            0.0,
            HEADER,
        );
        for (i, tr) in self.tracks.iter().enumerate() {
            let y = g.track_top(i);
            let badge = Rect::from_min_size(
                Pos2::new(rect.left() + 6.0, y + 4.0),
                Vec2::new(HEADER_W - 12.0, g.track_h - 8.0),
            );
            painter.rect_filled(badge, 5.0, if tr.audio { AUDIO } else { VIDEO }.gamma_multiply(0.55));
            painter.text(
                badge.center(),
                Align2::CENTER_CENTER,
                &tr.name,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
        }
        let _ = n;
    }
}
