//! Embed the built web app from `apps/web/build/index.html`.
//!
//! The web app builds with SvelteKit's `inline` bundle strategy, so its
//! scripts, styles and fonts are all inside `index.html` and the server needs
//! no other file. The script writes `assets.rs` into `OUT_DIR`. When the web
//! app is not built, `INDEX_HTML` is `None` and the server answers page
//! requests with a page that says how to build it.
//!
//! With `ARCL_REQUIRE_WEB=1` set, a missing web app fails the build instead.
//! The release workflow sets it, so no release binary ships without the app.
//!
//! `INDEX_SCRIPT_HASHES` lists the CSP source of each inline `<script>` in
//! the page, such as `'sha256-…'`, so the server's Content-Security-Policy
//! allows the app's own scripts and no others.

use std::path::PathBuf;
use std::{env, fs, io};

use sha2::{Digest, Sha256};

/// The standard base64 alphabet, which CSP hash sources use.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The variable that makes a missing web app a build error.
const REQUIRE_WEB: &str = "ARCL_REQUIRE_WEB";

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
    println!("cargo::rerun-if-env-changed={REQUIRE_WEB}");

    if !index.is_file() && env::var_os(REQUIRE_WEB).is_some_and(|value| value == "1") {
        eprintln!(
            "{REQUIRE_WEB}=1 and {} is missing. Run `pnpm install` and `pnpm --filter @arclightning/web build` first.",
            index.display()
        );
        std::process::exit(1);
    }

    let (value, hashes) = if index.is_file() {
        let html = fs::read_to_string(&index)?;
        let hashes = inline_scripts(&html)
            .map(|script| format!("'sha256-{}'", base64(&Sha256::digest(script.as_bytes()))))
            .collect::<Vec<_>>()
            .join(" ");
        let value = format!(
            "Some(include_bytes!({:?}))",
            fs::canonicalize(&index)?.display().to_string()
        );
        (value, hashes)
    } else {
        ("None".to_owned(), String::new())
    };
    let source = format!(
        "/// The built web app's `index.html`, or `None` when `apps/web` was not built.\n\
         static INDEX_HTML: Option<&[u8]> = {value};\n\
         /// The CSP hash source of each inline script in `INDEX_HTML`, separated by spaces.\n\
         static INDEX_SCRIPT_HASHES: &str = {hashes:?};\n"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("assets.rs"), source)
}

/// The text of each `<script>` element in `html` that has no `src`.
fn inline_scripts(html: &str) -> impl Iterator<Item = &str> {
    let mut rest = html;
    std::iter::from_fn(move || {
        loop {
            let start = rest.find("<script")?;
            let open_end = start + rest[start..].find('>')? + 1;
            let close = open_end + rest[open_end..].find("</script>")?;
            let tag = &rest[start..open_end];
            let text = &rest[open_end..close];
            rest = &rest[close..];
            if !tag.contains(" src=") {
                return Some(text);
            }
        }
    })
}

/// `bytes` in padded standard base64.
fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk
            .iter()
            .enumerate()
            .fold(0u32, |word, (i, byte)| word | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(BASE64[(word >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}
