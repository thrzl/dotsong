use std::{env, path::PathBuf};

const REQUIRED_KEYS: [&str; 4] = [
    "LASTFM_API_KEY",
    "LASTFM_API_SECRET",
    "LIBREFM_API_KEY",
    "LIBREFM_API_SECRET",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let env_path = manifest_dir.join("../.env");

    println!("cargo:rerun-if-changed={}", env_path.display());

    dotenvy::from_path(&env_path).ok();

    for key in REQUIRED_KEYS {
        let value =
            env::var(key).unwrap_or_else(|error| panic!("missing required env var {key}: {error}"));
        println!("cargo:rustc-env={key}={value}");
    }

    tauri_build::build()
}
