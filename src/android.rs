#![cfg(target_os = "android")]

#[unsafe(no_mangle)]
fn android_main(android_app: bevy::android::android_activity::AndroidApp) {
    bevy::android::ANDROID_APP.set(android_app).ok();
    Subsystem_for_Arxumbra::Entry_point();
}
