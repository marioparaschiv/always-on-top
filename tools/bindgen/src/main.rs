//! Regenerates `src/bindings.rs`, the WinUI 3 projection, from Windows App SDK `.winmd` files.
//!
//! cargo run --manifest-path tools/bindgen/Cargo.toml -- <winmd dir>
//!
//! See the README for where the `.winmd` files come from. Add a type to
//! `filter.txt` when the app needs one that is missing.

use std::path::Path;

fn main() {
    let winmd = std::env::args().nth(1).expect("pass the directory holding the .winmd files");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("src/bindings.rs");
    let filter = std::fs::read_to_string(root.join("tools/bindgen/filter.txt")).unwrap();

    let mut args: Vec<String> = ["--in", "default", &winmd, "--out", &out.display().to_string(), "--implement"]
        .map(String::from)
        .to_vec();
    // Reuse the `windows` crates for Windows.* so the types interoperate.
    for reference in [
        "windows-collections,flat,Windows.Foundation.Collections.IIterable",
        "windows-collections,flat,Windows.Foundation.Collections.IIterator",
        "windows-collections,flat,Windows.Foundation.Collections.IKeyValuePair",
        "windows-collections,flat,Windows.Foundation.Collections.IMap",
        "windows-collections,flat,Windows.Foundation.Collections.IMapView",
        "windows-collections,flat,Windows.Foundation.Collections.IVector",
        "windows-collections,flat,Windows.Foundation.Collections.IVectorView",
        "windows,skip-root,Windows",
    ] {
        args.extend(["--reference".into(), reference.into()]);
    }
    args.push("--filter".into());
    args.extend(filter.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from));

    let warnings = windows_bindgen::bindgen(&args);
    let skipped = warnings.to_string().lines().filter(|l| l.starts_with("skipping")).count();
    println!("wrote {} ({skipped} members skipped)", out.display());
}
