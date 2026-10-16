//! Export / import of everything callephiin stores (profiles, settings…), as one .zip.
//! The app folder is `%APPDATA%\callephiin` (Windows) or `~/Library/Application Support/callephiin` (macOS); logs and the image cache are left out.
use directories::ProjectDirs;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// `%APPDATA%\callephiin` (the folder that holds `config` and `logs`)
pub fn root() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "callephiin")?;
    if cfg!(windows) {
        // %APPDATA%\callephiin\config -> %APPDATA%\callephiin
        dirs.config_dir().parent().map(|p| p.to_path_buf())
    } else {
        // ~/Library/Application Support/callephiin (macOS), ~/.config/callephiin (Linux)
        Some(dirs.config_dir().to_path_buf())
    }
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap_or(&p).to_path_buf();
        let top = rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_string()).unwrap_or_default();
        if top == "logs" || top == "cache" || (rel.components().count() == 1 && p.extension().map_or(false, |x| x.eq_ignore_ascii_case("zip"))) {
            continue; // no logs, and no earlier exports
        }
        if p.is_dir() {
            collect(&p, root, out);
        } else {
            out.push(rel);
        }
    }
}

/// Writes `callephiin-config-<date>.zip` into the app folder and returns its path.
pub fn export() -> Result<PathBuf, String> {
    let root = root().ok_or("could not find the callephiin folder")?;
    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    if files.is_empty() {
        return Err("nothing to export yet".into());
    }
    let name = format!("callephiin-config-{}.zip", chrono::Local::now().format("%Y-%m-%d_%H%M%S"));
    let out = root.join(name);
    let file = fs::File::create(&out).map_err(|e| format!("could not create the zip: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for rel in files {
        let data = fs::read(root.join(&rel)).map_err(|e| format!("could not read {}: {e}", rel.display()))?;
        let name = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/");
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    log::debug!("config exported to {}", out.display());
    Ok(out)
}

/// Restores an export into the app folder. Returns how many files were written.
pub fn import(zip_path: &Path) -> Result<usize, String> {
    let root = root().ok_or("could not find the callephiin folder")?;
    let file = fs::File::open(zip_path).map_err(|e| format!("could not open the file: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "that is not a valid zip file".to_string())?;
    // check it really is a callephiin export before touching anything
    {
        let cfg_rel = crate::config::Config::path().and_then(|p| p.strip_prefix(&root).ok().map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/"))).unwrap_or_else(|| "config/config.json".into());
        let mut e = zip.by_name(&cfg_rel).map_err(|_| "this zip has no callephiin config in it".to_string())?;
        let mut s = String::new();
        e.read_to_string(&mut s).map_err(|e| e.to_string())?;
        serde_json::from_str::<crate::config::Config>(&s).map_err(|e| format!("the config in this zip is not valid: {e}"))?;
    }
    // keep the current config in case the import was a mistake
    if let Some(cur) = crate::config::Config::path().filter(|p| p.exists()) {
        crate::logged!(fs::copy(&cur, cur.with_file_name("config.before-import.json")));
    }
    let mut n = 0;
    for i in 0..zip.len() {
        let mut e = zip.by_index(i).map_err(|e| e.to_string())?;
        if e.is_dir() {
            continue;
        }
        let Some(rel) = e.enclosed_name() else { continue }; // refuses paths that escape the folder
        if rel.starts_with("logs") || rel.starts_with("cache") {
            continue;
        }
        let dest = root.join(&rel);
        if let Some(d) = dest.parent() {
            fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let mut data = Vec::new();
        e.read_to_end(&mut data).map_err(|e| e.to_string())?;
        fs::write(&dest, data).map_err(|e| format!("could not write {}: {e}", rel.display()))?;
        n += 1;
    }
    log::debug!("config imported from {} ({n} file(s))", zip_path.display());
    Ok(n)
}

/// Native "open file" dialog starting in the app folder; `None` if cancelled.
pub fn pick_zip() -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().add_filter("callephiin export", &["zip"]).set_title("Import config");
    if let Some(r) = root() {
        d = d.set_directory(r);
    }
    d.pick_file()
}
