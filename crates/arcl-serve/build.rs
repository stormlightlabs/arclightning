//! Embed the built web app from `apps/web/build/index.html`.
//!
//! The web app builds with SvelteKit's `inline` bundle strategy, so its
//! scripts, styles and fonts are all inside `index.html` and the server needs
//! no other file. The script writes `assets.rs` into `OUT_DIR`. When the web
//! app is not built, `INDEX_HTML` is `None` and the server answers page
//! requests with a page that says how to build it.

use std::path::PathBuf;
use std::{env, fs, io};

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let web = manifest.join("../../apps/web");
    let index = web.join("build/index.html");
    // Cargo reruns the script on every build while a watched path is
    // missing, so watch the nearest path that exists. A directory's mtime
    // changes when `vite build` creates the entry below it.
    let watched = [index.clone(), web.join("build"), web.clone()]
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| manifest.join("build.rs"));
    println!("cargo::rerun-if-changed={}", watched.display());

    let value = if index.is_file() {
        format!(
            "Some(include_bytes!({:?}))",
            fs::canonicalize(&index)?.display().to_string()
        )
    } else {
        "None".to_owned()
    };
    let source = format!(
        "/// The built web app's `index.html`, or `None` when `apps/web` was not built.\n\
         static INDEX_HTML: Option<&[u8]> = {value};\n"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("assets.rs"), source)
}
