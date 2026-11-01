// Entry desktop untuk testing cepat: `cargo run`
// Di Android entry-nya `android_main` di lib.rs
fn main() {
    #[cfg(not(target_os = "android"))]
    clipforge::run_desktop().unwrap();
}
