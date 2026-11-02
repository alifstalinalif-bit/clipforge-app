//! Helper khusus Android (JNI). Di platform lain semua fungsi no-op.

#[cfg(target_os = "android")]
mod imp {
    use jni::objects::{JObject, JString, JValue};
    use jni::{JNIEnv, JavaVM};
    use std::sync::OnceLock;
    use winit::platform::android::activity::AndroidApp;

    static APP: OnceLock<AndroidApp> = OnceLock::new();

    pub fn init(app: AndroidApp) {
        let _ = APP.set(app);
    }

    /// Jalankan kode JNI dengan activity. Exception Java dibersihkan, hasil None kalau gagal.
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

    /// Sembunyikan navigation bar + status bar (immersive sticky).
    pub fn set_immersive() {
        let _ = with_env(|env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let decor = env
                .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            // LAYOUT_STABLE | LAYOUT_HIDE_NAVIGATION | LAYOUT_FULLSCREEN
            // | HIDE_NAVIGATION | FULLSCREEN | IMMERSIVE_STICKY
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
                let s = env.new_string(p)?;
                let so: JObject = s.into();
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

    /// Durasi video/audio (detik) lewat MediaMetadataRetriever.
    pub fn media_duration(path: &str) -> Option<f64> {
        with_env(|env, _activity| {
            let r = env.new_object("android/media/MediaMetadataRetriever", "()V", &[])?;
            let p = env.new_string(path)?;
            let p_obj: JObject = p.into();
            if env
                .call_method(&r, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&p_obj)])
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
}

#[cfg(target_os = "android")]
pub use imp::*;

#[cfg(not(target_os = "android"))]
pub fn set_immersive() {}

#[cfg(not(target_os = "android"))]
pub fn has_media_permission() -> bool {
    true
}

#[cfg(not(target_os = "android"))]
pub fn request_media_permissions() {}

#[cfg(not(target_os = "android"))]
pub fn media_duration(_path: &str) -> Option<f64> {
    Some(10.0)
}
