fn main() {
    // App icon (resource 1) for the exe, windows and tray.
    embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
