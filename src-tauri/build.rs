use std::path::PathBuf;

fn load_dotenv() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    let env_path = PathBuf::from(manifest_dir)
        .parent()
        .expect("no parent dir")
        .join(".env");
    if !env_path.exists() {
        return;
    }
    let content = match std::fs::read_to_string(&env_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        println!("cargo:rustc-env={}={}", key, value);
    }
}

fn main() {
    load_dotenv();
    tauri_build::build()
}
