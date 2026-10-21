//! Files the user picks on a phone.
//!
//! On Android and iOS the open and save dialogs hand back URIs, not paths:
//! `content://…` (Android's Storage Access Framework) or `file://…` (iOS, a
//! security-scoped URL outside the app's sandbox). `std::fs` can't open
//! either, so the phone builds read and write them through tauri-plugin-fs,
//! which asks the OS for a file descriptor. Plain paths keep the desktop code.
//!
//! The extension checks stay. A `file://` URL must end in an allowed
//! extension, as a desktop path must. A `content://` URI often carries no
//! file name at all (`…/document/msf%3A1000000123`); when it shows one, the
//! extension must match, and when it doesn't, the URI is allowed, because
//! Android only opens a document the user picked in its own dialog.

// Desktop builds only use these in tests.
#![cfg_attr(not(mobile), allow(dead_code))]

use tauri_plugin_fs::FilePath;

/// The URI behind `path`, or `None` for an ordinary file path.
pub fn uri(path: &str) -> Option<FilePath> {
    match path.parse::<FilePath>() {
        Ok(u @ FilePath::Url(_)) => Some(u),
        _ => None,
    }
}

/// Whether the file name in `uri` is allowed: see the module docs.
pub fn name_allowed(uri: &FilePath, extensions: &[&str]) -> bool {
    let FilePath::Url(u) = uri else { return false };
    let last = u.path_segments().and_then(|mut s| s.next_back()).unwrap_or("");
    let decoded = percent_decode(last);
    // Android document ids look like `primary:Download/report.csv`.
    let name = decoded.rsplit(['/', ':']).next().unwrap_or("");
    match name.rsplit_once('.') {
        Some((_, ext)) => extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)),
        None => u.scheme() == "content",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (b.get(i + 1).and_then(|&c| hex(c)), b.get(i + 2).and_then(|&c| hex(c)))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Write `contents` to a picked URI, replacing what is there.
#[cfg(mobile)]
pub fn write(app: &tauri::AppHandle, uri: FilePath, contents: &str) -> crate::api::ApiResult<()> {
    use std::io::Write;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let shown = uri.to_string();
    let mut opts = OpenOptions::new();
    opts.write(true).truncate(true).create(true);
    // The file is closed at the end of the closure, before iOS gives up access.
    let written = (|| {
        let mut f = app.fs().open(uri.clone(), opts)?;
        f.write_all(contents.as_bytes())?;
        f.flush()
    })();
    #[cfg(target_os = "ios")]
    let _ = app.fs().stop_accessing_security_scoped_resource(uri);
    written.map_err(|e| format!("cannot write {shown}: {e}"))
}

/// Read a picked URI as UTF-8 text, refusing files over `limit` bytes.
#[cfg(mobile)]
pub fn read_text(app: &tauri::AppHandle, uri: FilePath, limit: u64) -> crate::api::ApiResult<String> {
    use std::io::Read;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let shown = uri.to_string();
    let mut opts = OpenOptions::new();
    opts.read(true);
    let mut buf = Vec::new();
    let read = app.fs().open(uri.clone(), opts).and_then(|f| f.take(limit + 1).read_to_end(&mut buf));
    #[cfg(target_os = "ios")]
    let _ = app.fs().stop_accessing_security_scoped_resource(uri);
    read.map_err(|e| format!("cannot read {shown}: {e}"))?;
    if buf.len() as u64 > limit {
        return Err(format!("the file is larger than {} MB", limit / (1024 * 1024)));
    }
    String::from_utf8(buf).map_err(|_| format!("cannot read {shown}: the file is not UTF-8 text"))
}

/// Read a picked URI as bytes, refusing files over `limit` bytes.
#[cfg(mobile)]
fn read_bytes(app: &tauri::AppHandle, uri: FilePath, limit: u64) -> crate::api::ApiResult<Vec<u8>> {
    use std::io::Read;
    use tauri_plugin_fs::{FsExt, OpenOptions};

    let shown = uri.to_string();
    let mut opts = OpenOptions::new();
    opts.read(true);
    let mut buf = Vec::new();
    let read = app.fs().open(uri.clone(), opts).and_then(|f| f.take(limit + 1).read_to_end(&mut buf));
    #[cfg(target_os = "ios")]
    let _ = app.fs().stop_accessing_security_scoped_resource(uri);
    read.map_err(|e| format!("cannot read {shown}: {e}"))?;
    if buf.len() as u64 > limit {
        return Err(format!("the file is larger than {} MB", limit / (1024 * 1024)));
    }
    Ok(buf)
}

/// A local path for a file the user picked to read: on a phone a picked URI
/// is copied into the app's cache first, so code that takes a path (Tally
/// import, accountant files) works unchanged. Desktop paths pass through.
pub fn local_input(app: &tauri::AppHandle, path: &str, extensions: &[&str], limit: u64) -> crate::api::ApiResult<std::path::PathBuf> {
    #[cfg(mobile)]
    if let Some(u) = uri(path) {
        use tauri::Manager;
        if !name_allowed(&u, extensions) {
            return Err(format!("pick a .{} file", extensions.join(" or .")));
        }
        let bytes = read_bytes(app, u, limit)?;
        let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("picked");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let ext = extensions.first().copied().unwrap_or("bin");
        let local = dir.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
        std::fs::write(&local, bytes).map_err(|e| e.to_string())?;
        return Ok(local);
    }
    let _ = (app, extensions, limit);
    Ok(std::path::PathBuf::from(path))
}

/// Where to write a file the user picked to save: a local scratch path on a
/// phone (copied to the picked URI by [`finish_output`]), else the path itself.
pub fn local_output(app: &tauri::AppHandle, path: &str, extension: &str) -> crate::api::ApiResult<std::path::PathBuf> {
    #[cfg(mobile)]
    if uri(path).is_some() {
        use tauri::Manager;
        let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("saving");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        return Ok(dir.join(format!("{}.{extension}", uuid::Uuid::new_v4())));
    }
    let _ = (app, extension);
    Ok(std::path::PathBuf::from(path))
}

/// Copy a file written by [`local_output`] to the picked URI (phones only).
pub fn finish_output(app: &tauri::AppHandle, path: &str, local: &std::path::Path) -> crate::api::ApiResult<()> {
    #[cfg(mobile)]
    if let Some(u) = uri(path) {
        use std::io::Write;
        use tauri_plugin_fs::{FsExt, OpenOptions};
        let bytes = std::fs::read(local).map_err(|e| e.to_string())?;
        let shown = u.to_string();
        let mut opts = OpenOptions::new();
        opts.write(true).truncate(true).create(true);
        let written = (|| {
            let mut f = app.fs().open(u.clone(), opts)?;
            f.write_all(&bytes)?;
            f.flush()
        })();
        #[cfg(target_os = "ios")]
        let _ = app.fs().stop_accessing_security_scoped_resource(u);
        let _ = std::fs::remove_file(local);
        return written.map_err(|e| format!("cannot write {shown}: {e}"));
    }
    let _ = (app, path, local);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str, ext: &[&str]) -> bool {
        name_allowed(&uri(s).expect("a URI"), ext)
    }

    #[test]
    fn paths_are_not_uris() {
        assert!(uri("/data/user/0/org.khataos.mobile/files/a.csv").is_none());
        assert!(uri("C:\\Users\\me\\a.csv").is_none());
        assert!(uri("report.csv").is_none());
        assert!(uri("content://com.android.providers.downloads.documents/document/msf%3A12").is_some());
        assert!(uri("file:///private/var/mobile/a.csv").is_some());
    }

    #[test]
    fn content_uris_check_a_visible_name() {
        let csv = &["csv"];
        assert!(ok("content://com.android.externalstorage.documents/document/primary%3ADownload%2Ftb.csv", csv));
        assert!(ok("content://com.android.externalstorage.documents/document/primary%3ADownload%2FTB.CSV", csv));
        assert!(!ok("content://com.android.externalstorage.documents/document/primary%3ADownload%2Ftb.sh", csv));
        // No name in the URI: Android only grants documents the user picked.
        assert!(ok("content://com.android.providers.downloads.documents/document/msf%3A1000000123", csv));
        assert!(ok("content://com.android.providers.downloads.documents/document/42", csv));
    }

    #[test]
    fn file_urls_need_the_extension() {
        let json = &["json", "xml"];
        assert!(ok("file:///private/var/mobile/Documents/gstr1.json", json));
        assert!(ok("file:///private/var/mobile/Documents/inv%20one.XML", json));
        assert!(!ok("file:///private/var/mobile/Documents/gstr1", json));
        assert!(!ok("file:///private/var/mobile/Documents/gstr1.csv", json));
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%2Fb%3Ac%20d"), "a/b:c d");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
    }
}
