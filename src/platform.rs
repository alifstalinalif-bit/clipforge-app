//! Helper Android (JNI): immersive, izin, durasi, audio (MediaPlayer), frame video (MediaMetadataRetriever).
//! Di platform lain semua fungsi no-op.

#[derive(Clone)]
pub struct AudioClip {
    pub id: u64,
    pub path: String,
    /// Posisi di file sumber (detik). Negatif = belum mulai, akan mulai dalam -pos detik.
    pub pos: f64,
    pub volume: f32,
}

#[derive(Clone)]
pub struct VideoReq {
    pub path: String,
    pub image: bool,
    pub t: f64,
}

pub struct VideoFrame {
    pub path: String,
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
}

#[cfg(target_os = "android")]
mod imp {
    use super::{AudioClip, VideoFrame, VideoReq};
    use jni::objects::{GlobalRef, JIntArray, JObject, JString, JValue};
    use jni::{JNIEnv, JavaVM};
    use std::collections::{HashMap, HashSet};
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::{Duration, Instant};
    use winit::platform::android::activity::AndroidApp;

    static APP: OnceLock<AndroidApp> = OnceLock::new();
    static FRAME: Mutex<Option<VideoFrame>> = Mutex::new(None);

    pub fn init(app: AndroidApp) {
        let _ = APP.set(app);
    }

    // ---------- kotak surat "yang terbaru menang" antar thread ----------

    struct Mailbox<T> {
        slot: Mutex<Option<T>>,
        cv: Condvar,
    }

    impl<T> Mailbox<T> {
        fn new() -> Self {
            Self { slot: Mutex::new(None), cv: Condvar::new() }
        }
        fn put(&self, v: T) {
            *self.slot.lock().unwrap() = Some(v);
            self.cv.notify_one();
        }
        fn take_timeout(&self, d: Duration) -> Option<T> {
            let mut g = self.slot.lock().unwrap();
            if g.is_none() {
                g = self.cv.wait_timeout(g, d).unwrap().0;
            }
            g.take()
        }
    }

    static VIDEO_BOX: OnceLock<Mailbox<VideoReq>> = OnceLock::new();
    static AUDIO_BOX: OnceLock<Mailbox<(bool, Vec<AudioClip>)>> = OnceLock::new();

    fn video_box() -> &'static Mailbox<VideoReq> {
        VIDEO_BOX.get_or_init(Mailbox::new)
    }
    fn audio_box() -> &'static Mailbox<(bool, Vec<AudioClip>)> {
        AUDIO_BOX.get_or_init(Mailbox::new)
    }

    // ---------- helper JNI ----------

    fn jstr<'a>(env: &mut JNIEnv<'a>, s: &str) -> jni::errors::Result<JObject<'a>> {
        Ok(env.new_string(s)?.into())
    }

    fn clear_exc(env: &mut JNIEnv) {
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
    }

    fn call_i(env: &mut JNIEnv, obj: &GlobalRef, name: &str) -> i32 {
        let r = env
            .call_method(obj, name, "()I", &[])
            .ok()
            .and_then(|v| v.i().ok())
            .unwrap_or(0);
        clear_exc(env);
        r
    }

    fn call_b(env: &mut JNIEnv, obj: &GlobalRef, name: &str) -> bool {
        let r = env
            .call_method(obj, name, "()Z", &[])
            .ok()
            .and_then(|v| v.z().ok())
            .unwrap_or(false);
        clear_exc(env);
        r
    }

    fn call_v(env: &mut JNIEnv, obj: &GlobalRef, name: &str) {
        let _ = env.call_method(obj, name, "()V", &[]);
        clear_exc(env);
    }

    /// Jalankan kode JNI di thread utama dengan activity. Exception Java dibersihkan.
    fn with_env<R>(f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>) -> Option<R> {
        let app = APP.get()?;
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut jni::sys::JavaVM) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jni::sys::jobject) };
        let result = f(&mut *env, &activity);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
            return None;
        }
        result.ok()
    }

    pub fn set_immersive() {
        let _ = with_env(|env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let decor = env
                .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            let flags: i32 = 0x100 | 0x200 | 0x400 | 0x2 | 0x4 | 0x1000;
            env.call_method(&decor, "setSystemUiVisibility", "(I)V", &[JValue::Int(flags)])?;
            Ok(())
        });
    }

    const PERMS: [&str; 4] = [
        "android.permission.READ_MEDIA_VIDEO",
        "android.permission.READ_MEDIA_AUDIO",
        "android.permission.READ_MEDIA_IMAGES",
        "android.permission.READ_EXTERNAL_STORAGE",
    ];

    pub fn has_media_permission() -> bool {
        with_env(|env, activity| {
            let mut ok = false;
            for p in PERMS {
                let so = jstr(env, p)?;
                let r = env
                    .call_method(
                        activity,
                        "checkSelfPermission",
                        "(Ljava/lang/String;)I",
                        &[JValue::Object(&so)],
                    )?
                    .i()?;
                if r == 0 {
                    ok = true;
                }
            }
            Ok(ok)
        })
        .unwrap_or(false)
    }

    pub fn request_media_permissions() {
        let _ = with_env(|env, activity| {
            let arr = env.new_object_array(PERMS.len() as i32, "java/lang/String", JObject::null())?;
            for (i, p) in PERMS.iter().enumerate() {
                let s = env.new_string(*p)?;
                env.set_object_array_element(&arr, i as i32, &s)?;
            }
            let arr_obj: JObject = arr.into();
            env.call_method(
                activity,
                "requestPermissions",
                "([Ljava/lang/String;I)V",
                &[JValue::Object(&arr_obj), JValue::Int(1001)],
            )?;
            Ok(())
        });
    }

    pub fn media_duration(path: &str) -> Option<f64> {
        with_env(|env, _activity| {
            let r = env.new_object("android/media/MediaMetadataRetriever", "()V", &[])?;
            let p = jstr(env, path)?;
            if env
                .call_method(&r, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&p)])
                .is_err()
            {
                let _ = env.exception_clear();
                return Ok(None);
            }
            let d = env
                .call_method(&r, "extractMetadata", "(I)Ljava/lang/String;", &[JValue::Int(9)])?
                .l()?;
            let _ = env.call_method(&r, "release", "()V", &[]);
            if d.is_null() {
                return Ok(None);
            }
            let js = JString::from(d);
            let s: String = env.get_string(&js)?.into();
            Ok(s.trim().parse::<f64>().ok().map(|ms| ms / 1000.0))
        })
        .flatten()
    }

    // ---------- API untuk thread UI ----------

    pub fn av_init(ctx: &egui::Context) {
        static STARTED: OnceLock<()> = OnceLock::new();
        let Some(app) = APP.get() else { return };
        if STARTED.set(()).is_err() {
            return;
        }
        let vm_ptr = app.vm_as_ptr() as usize;
        let c = ctx.clone();
        std::thread::spawn(move || {
            if let Ok(vm) = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) } {
                video_worker(vm, c);
            }
        });
        std::thread::spawn(move || {
            if let Ok(vm) = unsafe { JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) } {
                audio_worker(vm);
            }
        });
    }

    pub fn audio_sync(playing: bool, clips: Vec<AudioClip>) {
        audio_box().put((playing, clips));
    }

    pub fn video_request(req: VideoReq) {
        video_box().put(req);
    }

    pub fn video_take() -> Option<VideoFrame> {
        FRAME.lock().unwrap().take()
    }

    // ---------- worker video ----------

    fn video_worker(vm: JavaVM, ctx: egui::Context) {
        let Ok(mut env) = vm.attach_current_thread_permanently() else { return };
        let mut cache: HashMap<String, GlobalRef> = HashMap::new();
        let mb = video_box();
        loop {
            let Some(req) = mb.take_timeout(Duration::from_secs(30)) else { continue };
            if cache.len() > 4 {
                cache.clear();
            }
            let res = env.with_local_frame(48, |env| decode(env, &mut cache, &req));
            clear_exc(&mut env);
            if let Ok(Some(frame)) = res {
                *FRAME.lock().unwrap() = Some(frame);
                ctx.request_repaint();
            }
        }
    }

    fn meta_int(env: &mut JNIEnv, r: &GlobalRef, key: i32) -> Option<i32> {
        let s = env
            .call_method(r, "extractMetadata", "(I)Ljava/lang/String;", &[JValue::Int(key)])
            .ok()?
            .l()
            .ok()?;
        if s.is_null() {
            return None;
        }
        let js = JString::from(s);
        let st: String = env.get_string(&js).ok()?.into();
        st.trim().parse().ok()
    }

    fn decode(
        env: &mut JNIEnv,
        cache: &mut HashMap<String, GlobalRef>,
        req: &VideoReq,
    ) -> jni::errors::Result<Option<VideoFrame>> {
        let bmp: JObject = if req.image {
            let opts = env.new_object("android/graphics/BitmapFactory$Options", "()V", &[])?;
            env.set_field(&opts, "inSampleSize", "I", JValue::Int(4))?;
            let p = jstr(env, &req.path)?;
            env.call_static_method(
                "android/graphics/BitmapFactory",
                "decodeFile",
                "(Ljava/lang/String;Landroid/graphics/BitmapFactory$Options;)Landroid/graphics/Bitmap;",
                &[JValue::Object(&p), JValue::Object(&opts)],
            )?
            .l()?
        } else {
            if !cache.contains_key(&req.path) {
                let r = env.new_object("android/media/MediaMetadataRetriever", "()V", &[])?;
                let p = jstr(env, &req.path)?;
                if env
                    .call_method(&r, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&p)])
                    .is_err()
                {
                    env.exception_clear()?;
                    return Ok(None);
                }
                let g = env.new_global_ref(&r)?;
                cache.insert(req.path.clone(), g);
            }
            let r = cache.get(&req.path).unwrap().clone();
            let mut w = meta_int(env, &r, 18).unwrap_or(1280).max(1);
            let mut h = meta_int(env, &r, 19).unwrap_or(720).max(1);
            let rot = meta_int(env, &r, 24).unwrap_or(0);
            if rot == 90 || rot == 270 {
                std::mem::swap(&mut w, &mut h);
            }
            let tw: i32 = 640;
            let th: i32 = (((tw as f32) * (h as f32) / (w as f32)) as i32).max(2) & !1;
            let us = (req.t.max(0.0) * 1_000_000.0) as i64;
            let mut b = env
                .call_method(
                    &r,
                    "getScaledFrameAtTime",
                    "(JIII)Landroid/graphics/Bitmap;",
                    &[JValue::Long(us), JValue::Int(3), JValue::Int(tw), JValue::Int(th)],
                )?
                .l()?;
            if b.is_null() {
                b = env
                    .call_method(
                        &r,
                        "getFrameAtTime",
                        "(JI)Landroid/graphics/Bitmap;",
                        &[JValue::Long(us), JValue::Int(3)],
                    )?
                    .l()?;
            }
            b
        };
        if bmp.is_null() {
            return Ok(None);
        }
        let w = env.call_method(&bmp, "getWidth", "()I", &[])?.i()?;
        let h = env.call_method(&bmp, "getHeight", "()I", &[])?.i()?;
        if w <= 0 || h <= 0 {
            return Ok(None);
        }
        let n = (w as usize) * (h as usize);
        let arr_obj: JObject = env.new_int_array(n as i32)?.into();
        env.call_method(
            &bmp,
            "getPixels",
            "([IIIIIII)V",
            &[
                JValue::Object(&arr_obj),
                JValue::Int(0),
                JValue::Int(w),
                JValue::Int(0),
                JValue::Int(0),
                JValue::Int(w),
                JValue::Int(h),
            ],
        )?;
        let arr = JIntArray::from(arr_obj);
        let mut buf = vec![0i32; n];
        env.get_int_array_region(&arr, 0, &mut buf)?;
        let _ = env.call_method(&bmp, "recycle", "()V", &[]);
        let mut rgba = Vec::with_capacity(n * 4);
        for v in buf {
            rgba.extend_from_slice(&[((v >> 16) & 255) as u8, ((v >> 8) & 255) as u8, (v & 255) as u8, 255]);
        }
        Ok(Some(VideoFrame { path: req.path.clone(), size: [w as usize, h as usize], rgba }))
    }

    // ---------- worker audio ----------

    struct Pl {
        obj: GlobalRef,
        dur_ms: i32,
        positioned: bool,
        last_seek: Instant,
        vol: f32,
    }

    fn audio_worker(vm: JavaVM) {
        let Ok(mut env) = vm.attach_current_thread_permanently() else { return };
        let mb = audio_box();
        let mut cur: (bool, Vec<AudioClip>) = (false, vec![]);
        let mut last_put = Instant::now();
        let mut players: HashMap<u64, Option<Pl>> = HashMap::new();
        loop {
            if let Some(s) = mb.take_timeout(Duration::from_millis(30)) {
                cur = s;
                last_put = Instant::now();
            }
            // Kalau UI berhenti mengirim status (app di background), anggap pause.
            if cur.0 && last_put.elapsed() > Duration::from_millis(1200) {
                cur.0 = false;
            }
            let _ = env.with_local_frame(48, |env| -> jni::errors::Result<()> {
                reconcile(env, &cur, &mut players);
                Ok(())
            });
            clear_exc(&mut env);
        }
    }

    fn release(env: &mut JNIEnv, p: &Pl) {
        call_v(env, &p.obj, "release");
    }

    fn create_player(env: &mut JNIEnv, path: &str) -> Option<Pl> {
        let r: jni::errors::Result<Pl> = (|| {
            let mp = env.new_object("android/media/MediaPlayer", "()V", &[])?;
            let s = jstr(env, path)?;
            env.call_method(&mp, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&s)])?;
            env.call_method(&mp, "prepare", "()V", &[])?;
            let dur_ms = env.call_method(&mp, "getDuration", "()I", &[])?.i()?;
            let obj = env.new_global_ref(&mp)?;
            Ok(Pl { obj, dur_ms, positioned: false, last_seek: Instant::now().checked_sub(Duration::from_secs(5)).unwrap_or_else(Instant::now), vol: -1.0 })
        })();
        match r {
            Ok(p) => Some(p),
            Err(_) => {
                clear_exc(env);
                None
            }
        }
    }

    fn reconcile(
        env: &mut JNIEnv,
        cur: &(bool, Vec<AudioClip>),
        players: &mut HashMap<u64, Option<Pl>>,
    ) {
        let (playing, clips) = cur;
        let live: HashSet<u64> = clips.iter().map(|c| c.id).collect();
        let stale: Vec<u64> = players.keys().copied().filter(|k| !live.contains(k)).collect();
        for k in stale {
            if let Some(Some(p)) = players.remove(&k) {
                release(env, &p);
            }
        }

        for c in clips {
            let entry = players.entry(c.id).or_insert_with(|| create_player(env, &c.path));
            let Some(p) = entry.as_mut() else { continue };
            if c.pos < 0.0 {
                continue; // sudah disiapkan, belum waktunya
            }
            let target = (c.pos * 1000.0) as i32;
            let now_ms = call_i(env, &p.obj, "getCurrentPosition");
            let is_playing = call_b(env, &p.obj, "isPlaying");

            let drift = (now_ms - target).abs();
            let need_seek = !p.positioned || (*playing && drift > 350);
            if need_seek && p.last_seek.elapsed() > Duration::from_millis(450) {
                let _ = env.call_method(&p.obj, "seekTo", "(JI)V", &[JValue::Long(target as i64), JValue::Int(3)]);
                clear_exc(env);
                p.positioned = true;
                p.last_seek = Instant::now();
            }

            if (p.vol - c.volume).abs() > 0.001 {
                let v = c.volume.clamp(0.0, 1.0);
                let _ = env.call_method(&p.obj, "setVolume", "(FF)V", &[JValue::Float(v), JValue::Float(v)]);
                clear_exc(env);
                p.vol = c.volume;
            }

            let past_end = p.dur_ms > 0 && target >= p.dur_ms - 30;
            if *playing && !is_playing && !past_end {
                call_v(env, &p.obj, "start");
            } else if (!*playing || past_end) && is_playing {
                call_v(env, &p.obj, "pause");
            }
        }
    }
}

#[cfg(target_os = "android")]
pub use imp::*;

#[cfg(not(target_os = "android"))]
mod stub {
    use super::{AudioClip, VideoFrame, VideoReq};
    pub fn set_immersive() {}
    pub fn has_media_permission() -> bool {
        true
    }
    pub fn request_media_permissions() {}
    pub fn media_duration(_path: &str) -> Option<f64> {
        Some(10.0)
    }
    pub fn av_init(_ctx: &egui::Context) {}
    pub fn audio_sync(_playing: bool, _clips: Vec<AudioClip>) {}
    pub fn video_request(_req: VideoReq) {}
    pub fn video_take() -> Option<VideoFrame> {
        None
    }
}

#[cfg(not(target_os = "android"))]
pub use stub::*;
