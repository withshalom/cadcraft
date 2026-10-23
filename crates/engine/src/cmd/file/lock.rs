//! The `.cdl` lock beside an open `.cad` drawing (docs/cad-format.md §10): BlackCAD's lock file
//! under CADCraft's name, from the open `cad-native` crate.
//!
//! Opening a `.cad` drawing from a file takes its lock; if another copy of CADCraft (or
//! BlackCAD) holds it with a live heartbeat, the drawing opens read-only: saving to that file is
//! refused, Save As to another name works. The host calls [`refresh_locks`] regularly (the UI does
//! every frame) to keep the heartbeat live; a lock left behind by a crash goes stale after two
//! minutes and is taken over. Closing the drawing removes the lock. Locks are advisory, and the
//! web build, which has no files, has none.

use std::sync::{Arc, Mutex};

use cad_native::lock::{self as nl, LockInfo, LockKind};

use crate::{Result, Session};

/// The lock state of one open drawing.
#[derive(Clone, Debug, Default)]
pub enum DocLock {
    /// Not a `.cad` file on disk (or locks aren't available).
    #[default]
    None,
    /// This copy holds the lock. Shared by the drawings of this session open on the same file.
    Held(Arc<Mutex<Held>>),
    /// Someone else holds it: the drawing is read-only.
    ReadOnly(LockInfo),
}

impl DocLock {
    /// Who holds the drawing's file when it is read-only here.
    pub fn holder(&self) -> Option<&LockInfo> {
        match self {
            DocLock::ReadOnly(i) => Some(i),
            _ => None,
        }
    }
}

/// A lock this copy holds, and when its heartbeat was last written.
#[derive(Debug)]
pub struct Held {
    lock: nl::DrawingLock,
    #[cfg(not(target_arch = "wasm32"))]
    refreshed: std::time::Instant,
}

/// Whether `path` is a `.cad` drawing (CADCraft locks those).
pub fn is_cad(path: &str) -> bool {
    std::path::Path::new(path).extension().is_some_and(|e| e.eq_ignore_ascii_case(LockKind::CADCRAFT.native))
}

/// The program named in locks this copy writes.
fn app() -> String {
    format!("CADCraft {}", env!("CARGO_PKG_VERSION"))
}

/// The lock file of `path`: `part.cad` → `part.cdl`.
pub fn lock_path(path: &str) -> std::path::PathBuf {
    nl::lock_path_for(std::path::Path::new(path), LockKind::CADCRAFT)
}

/// Take the lock of `path` for drawing `me` of the session: shared with another drawing of the
/// session open on the same file, held by someone else (read-only), or new.
#[cfg(not(target_arch = "wasm32"))]
fn acquire(s: &Session, path: &str, me: Option<usize>) -> std::result::Result<DocLock, String> {
    if !is_cad(path) {
        return Ok(DocLock::None);
    }
    let want = lock_path(path);
    for (i, st) in s.docs.iter().enumerate() {
        if Some(i) == me {
            continue;
        }
        if let DocLock::Held(h) = &st.lock
            && h.lock().is_ok_and(|h| h.lock.path() == want)
        {
            return Ok(DocLock::Held(h.clone()));
        }
    }
    take(path)
}

/// Take the lock of `path` from the file system (no sharing with the session's drawings).
#[cfg(not(target_arch = "wasm32"))]
fn take(path: &str) -> std::result::Result<DocLock, String> {
    match nl::acquire_for(path, nl::user_name(), &app(), LockKind::CADCRAFT).map_err(|e| e.to_string())? {
        nl::Acquire::Acquired(lock) => Ok(DocLock::Held(Arc::new(Mutex::new(Held { lock, refreshed: std::time::Instant::now() })))),
        nl::Acquire::HeldBy(info) => Ok(DocLock::ReadOnly(info)),
    }
}

/// Write the heartbeat of a held lock now; false when it isn't this copy's any more (taken over
/// while nobody refreshed it, or removed).
#[cfg(not(target_arch = "wasm32"))]
fn still_ours(h: &Mutex<Held>) -> bool {
    let Ok(mut held) = h.lock() else { return false };
    let ok = held.lock.refresh().is_ok();
    if ok {
        held.refreshed = std::time::Instant::now();
    }
    ok
}

#[cfg(target_arch = "wasm32")]
fn acquire(_: &Session, _: &str, _: Option<usize>) -> std::result::Result<DocLock, String> {
    Ok(DocLock::None)
}
#[cfg(target_arch = "wasm32")]
fn take(_: &str) -> std::result::Result<DocLock, String> {
    Ok(DocLock::None)
}
#[cfg(target_arch = "wasm32")]
fn still_ours(_: &Mutex<Held>) -> bool {
    true
}

/// `secs` since 1970 as `YYYY-MM-DD HH:MM UTC`.
fn utc(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let (h, m) = (secs % 86_400 / 3600, secs % 3600 / 60);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

/// Who holds a file, as a sentence part: "shalom on shop-pc (CADCraft 0.10.0) since …".
pub fn describe(i: &LockInfo) -> String {
    let host = if i.host.is_empty() { String::new() } else { format!(" on {}", i.host) };
    let app = if i.app.is_empty() { String::new() } else { format!(" ({})", i.app) };
    let since = if i.opened > 0 { format!(" since {}", utc(i.opened)) } else { String::new() };
    format!("{}{host}{app}{since}", if i.user.is_empty() { "someone" } else { &i.user })
}

/// After drawing `i` was opened from a file: take its lock, or open it read-only and say who has
/// it.
pub(crate) fn after_open(s: &mut Session, i: usize) {
    let Some(path) = s.docs.get(i).and_then(|st| st.path.clone()) else { return };
    if !is_cad(&path) {
        return;
    }
    let lock = acquire(s, &path, Some(i));
    match lock {
        Ok(l) => {
            if let Some(info) = l.holder() {
                let name = super::file_name(&path);
                let msg =
                    format!("{name} is open by {}: it was opened read-only. Save it under another name (SAVEAS) to keep changes.", describe(info));
                s.echo(msg);
            }
            if let Some(st) = s.docs.get_mut(i) {
                st.lock = l;
            }
        }
        Err(e) => s.echo(format!("{}: could not take the drawing's lock ({e}); others aren't told it is open.", super::file_name(&path))),
    }
}

/// Before the active drawing is saved as a drawing file at `path`: the lock it will have once
/// saved there, or why it can't be saved there.
pub(crate) fn before_save(s: &Session, cmd: &str, path: &str) -> Result<DocLock> {
    let st = s.state()?;
    let same = st.path.as_deref() == Some(path);
    if same {
        match &st.lock {
            DocLock::ReadOnly(info) => {
                return Err(super::bad(
                    cmd,
                    format!("{} is open by {}, so this copy is read-only. Save under another name (SAVEAS).", super::file_name(path), describe(info)),
                ));
            }
            // Saving checks the lock is still this copy's, whether or not the host keeps the
            // heartbeat (a script or the MCP may not): a file taken over isn't saved over.
            DocLock::Held(h) if still_ours(h) => return Ok(st.lock.clone()),
            DocLock::Held(_) => {
                return match take(path) {
                    Ok(DocLock::ReadOnly(info)) => Err(super::bad(
                        cmd,
                        format!(
                            "{} was taken over by {} while this copy's lock lapsed; it can't be saved over. Save under another name (SAVEAS).",
                            super::file_name(path),
                            describe(&info)
                        ),
                    )),
                    Ok(l) => Ok(l),
                    Err(_) => Ok(DocLock::None),
                };
            }
            DocLock::None => {}
        }
    }
    if !is_cad(path) {
        return Ok(DocLock::None);
    }
    match acquire(s, path, Some(s.active)) {
        Ok(DocLock::ReadOnly(info)) => {
            Err(super::bad(cmd, format!("{} is open by {}; it can't be saved over.", super::file_name(path), describe(&info))))
        }
        Ok(l) => Ok(l),
        // No lock (a folder where it can't be written): saving tells whether the file can be.
        Err(_) => Ok(DocLock::None),
    }
}

/// How often a held lock's heartbeat is written.
pub const HEARTBEAT: std::time::Duration = std::time::Duration::from_secs(nl::HEARTBEAT_SECS);

/// Write the heartbeat of every lock due for it (every [`HEARTBEAT`]). A lock that was taken
/// over while this copy wasn't refreshing it (or removed) is taken again if it is free, and
/// otherwise the drawing becomes read-only, with a message. Returns how long until the next
/// heartbeat is due (`None`: no locks held).
#[cfg(not(target_arch = "wasm32"))]
pub fn refresh_locks(s: &mut Session) -> Option<std::time::Duration> {
    refresh_locks_at(s, false)
}

#[cfg(target_arch = "wasm32")]
pub fn refresh_locks(_: &mut Session) -> Option<std::time::Duration> {
    None
}

/// [`refresh_locks`], refreshing every held lock now when `force` is set.
#[cfg(not(target_arch = "wasm32"))]
pub fn refresh_locks_at(s: &mut Session, force: bool) -> Option<std::time::Duration> {
    let mut next: Option<std::time::Duration> = None;
    let mut lost: Vec<Arc<Mutex<Held>>> = Vec::new();
    for st in &s.docs {
        let DocLock::Held(h) = &st.lock else { continue };
        if lost.iter().any(|l| Arc::ptr_eq(l, h)) {
            continue;
        }
        let Ok(mut held) = h.lock() else { continue };
        let age = held.refreshed.elapsed();
        if force || age >= HEARTBEAT {
            if held.lock.refresh().is_err() {
                lost.push(h.clone());
                continue;
            }
            held.refreshed = std::time::Instant::now();
            next = Some(next.map_or(HEARTBEAT, |n| n.min(HEARTBEAT)));
        } else {
            let wait = HEARTBEAT.saturating_sub(age);
            next = Some(next.map_or(wait, |n| n.min(wait)));
        }
    }
    for old in lost {
        let docs: Vec<usize> = (0..s.docs.len()).filter(|&i| matches!(&s.docs[i].lock, DocLock::Held(h) if Arc::ptr_eq(h, &old))).collect();
        let Some(path) = docs.first().and_then(|&i| s.docs[i].path.clone()) else { continue };
        for &i in &docs {
            s.docs[i].lock = DocLock::None;
        }
        drop(old);
        let again = acquire(s, &path, None).unwrap_or_default();
        if let Some(info) = again.holder() {
            let msg = format!(
                "{} was taken over by {} (this copy's lock lapsed); it is read-only now. Save it under another name (SAVEAS) to keep changes.",
                super::file_name(&path),
                describe(info)
            );
            s.echo(msg);
        }
        for &i in &docs {
            s.docs[i].lock = again.clone();
        }
    }
    next
}

/// Release every lock this session holds (the app is closing).
pub fn release_locks(s: &mut Session) {
    for st in &mut s.docs {
        st.lock = DocLock::None;
    }
}

#[cfg(test)]
mod tests {
    use super::utc;

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc(1_792_678_200), "2026-10-22 14:10 UTC");
    }
}
