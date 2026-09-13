//! Bakes the optional Supabase configuration into the library.
//!
//! If a `.env` file sits next to `Cargo.toml`, its `SUPABASE_URL` and
//! `SUPABASE_ANON_KEY` entries are re-emitted as the compile-time env vars
//! `SUCCESS_SUPABASE_URL` and `SUCCESS_SUPABASE_ANON_KEY`, which
//! `sync_default()` reads via `option_env!`. A missing file or a missing key
//! simply means no default is baked in; the build never fails because of it.

use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=.env");

    let Ok(contents) = fs::read_to_string(".env") else {
        return;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "SUPABASE_URL" => {
                println!("cargo:rustc-env=SUCCESS_SUPABASE_URL={}", value.trim());
            }
            "SUPABASE_ANON_KEY" => {
                println!("cargo:rustc-env=SUCCESS_SUPABASE_ANON_KEY={}", value.trim());
            }
            _ => {}
        }
    }
}
