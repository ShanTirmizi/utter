//! Writing a file so that a crash or power cut leaves either the old
//! contents or the new, never half of one.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Held while writing, so two writes at once can never share a temporary
/// file and mix their bytes.
static WRITING: Mutex<()> = Mutex::new(());

/// The temporary file `file` is written through, next to it so the rename
/// stays on one disk.
pub fn partial_path(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    file.with_file_name(name)
}

/// Replace `file` with `bytes`: write a temporary file in the same folder,
/// flush it to disk, rename it over `file`, then flush the folder so the
/// rename itself survives a power cut. `mode` sets the file's permissions;
/// `None` keeps the usual ones.
pub fn write(file: &Path, bytes: &[u8], mode: Option<u32>) -> Result<(), String> {
    let partial = partial_path(file);
    let _writing = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        if let Some(mode) = mode {
            options.mode(mode);
        }
        let mut out = options.open(&partial)?;
        if let Some(mode) = mode {
            // `mode` only applies when the file is created; a leftover
            // temporary file from an earlier crash keeps its own.
            out.set_permissions(fs::Permissions::from_mode(mode))?;
        }
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        fs::rename(&partial, file)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&partial);
        return Err(e.to_string());
    }
    sync_dir(file);
    Ok(())
}

/// Flush the folder holding `file`, so a rename in it is on disk. Best
/// effort: the file itself is already safe, and not every file system
/// allows it.
pub fn sync_dir(file: &Path) {
    if let Some(dir) = file.parent() {
        let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
        if let Ok(handle) = File::open(dir) {
            let _ = handle.sync_all();
        }
    }
}
