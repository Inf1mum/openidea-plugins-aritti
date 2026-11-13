use std::{fs, path::Path};

// The IDE keeps a plugin's web files while its wasm is unchanged; folding a hash of web/ into the wasm makes a rebuild after a web edit reload them.
fn main() {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut files = Vec::new();
    collect(Path::new("web"), &mut files);
    files.sort();
    for file in files {
        for byte in file.to_string_lossy().bytes().chain(fs::read(&file).unwrap_or_default()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    println!("cargo:rustc-env=JPA_WEB_STAMP={hash:016x}");
    println!("cargo:rerun-if-changed=web");
}

fn collect(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
