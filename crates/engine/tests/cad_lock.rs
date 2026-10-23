//! The `.cdl` lock beside an open `.cad` drawing (docs/cad-format.md §10), with the real file
//! formats installed.

use std::path::{Path, PathBuf};

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{self, lock};
use serde_json::json;

fn hooks() {
    file::set_io(file::IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cadcraft-lock-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A `.cad` drawing with one circle at `dir/name`, saved by a session that has closed again.
fn saved_cad(dir: &Path, name: &str) -> String {
    hooks();
    let path = s(&dir.join(name));
    let mut session = Session::new();
    session.cmdline("circle 0,0 5").unwrap();
    session.execute("saveas", &json!({ "path": path })).unwrap();
    drop(session);
    assert!(!Path::new(&lock::lock_path(&path)).exists(), "the saving session released its lock");
    path
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

/// A lock someone else holds on `path`, with its heartbeat `age` seconds old.
fn theirs(path: &str, age: u64) {
    let t = now() - age;
    std::fs::write(
        lock::lock_path(path),
        format!("cdl 1\nuser=alex\nhost=other-pc\npid=1\napp=CADCraft 0.10.0\nsession=feedbeef\nopened={t}\nheartbeat={t}\n"),
    )
    .unwrap();
}

fn lock_text(path: &str) -> String {
    std::fs::read_to_string(lock::lock_path(path)).unwrap_or_default()
}

#[test]
fn opening_a_cad_drawing_locks_it_until_it_closes() {
    let d = dir("open");
    let p = saved_cad(&d, "part.cad");
    assert_eq!(lock::lock_path(&p), d.join("part.cdl"));
    let mut session = Session::new();
    let r = session.execute("open", &json!({ "path": p })).unwrap();
    assert!(r.get("readOnly").is_none(), "{r}");
    let text = lock_text(&p);
    assert!(text.starts_with("cdl 1\n") && text.contains("app=CADCraft"), "{text}");
    assert_eq!(session.state().unwrap().doc.model.len(), 1, "the .cad drawing opened");
    session.execute("close", &json!({})).unwrap();
    assert!(!d.join("part.cdl").exists());
}

#[test]
fn a_drawing_someone_else_has_open_opens_read_only() {
    let d = dir("theirs");
    let p = saved_cad(&d, "part.cad");
    theirs(&p, 5);
    let before = std::fs::read(&p).unwrap();
    let mut session = Session::new();
    let r = session.execute("open", &json!({ "path": p })).unwrap();
    assert_eq!(r["readOnly"]["user"], "alex", "{r}");
    assert!(session.log.iter().any(|l| l.contains("alex on other-pc") && l.contains("read-only")), "{:?}", session.log);
    session.cmdline("line 0,0 1,1").unwrap();
    session.cmdline("").unwrap();
    let e = session.execute("qsave", &json!({})).unwrap_err().to_string();
    assert!(e.contains("read-only") && e.contains("SAVEAS"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), before, "their file is untouched");
    // Save As another name works, and that file is this copy's.
    let other = s(&d.join("mine.cad"));
    session.execute("saveas", &json!({ "path": other })).unwrap();
    assert!(session.state().unwrap().lock.holder().is_none());
    assert!(lock_text(&other).contains("app=CADCraft") && !lock_text(&other).contains("alex"));
    assert!(lock_text(&p).contains("user=alex"), "their lock stays");
}

#[test]
fn a_stale_lock_is_taken_over() {
    let d = dir("stale");
    let p = saved_cad(&d, "part.cad");
    theirs(&p, 600);
    let mut session = Session::new();
    let r = session.execute("open", &json!({ "path": p })).unwrap();
    assert!(r.get("readOnly").is_none(), "{r}");
    assert!(!lock_text(&p).contains("alex"));
    session.cmdline("line 0,0 1,1").unwrap();
    session.cmdline("").unwrap();
    session.execute("qsave", &json!({})).unwrap();
}

#[test]
fn saving_over_a_file_someone_has_open_is_refused() {
    let d = dir("over");
    let target = s(&d.join("shared.cad"));
    theirs(&target, 5);
    hooks();
    let mut session = Session::new();
    session.cmdline("circle 0,0 5").unwrap();
    let e = session.execute("saveas", &json!({ "path": target })).unwrap_err().to_string();
    assert!(e.contains("alex") && e.contains("can't be saved over"), "{e}");
    assert!(!Path::new(&target).exists());
}

#[test]
fn saving_as_another_format_releases_the_lock() {
    let d = dir("dxf");
    let p = saved_cad(&d, "part.cad");
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    assert!(d.join("part.cdl").exists());
    session.execute("saveas", &json!({ "path": s(&d.join("part.dxf")) })).unwrap();
    assert!(!d.join("part.cdl").exists());
    // An export (not a drawing file) keeps the drawing, and its lock, where they are.
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    session.execute("saveas", &json!({ "path": s(&d.join("part.svg")) })).unwrap();
    assert!(d.join("part.cdl").exists());
}

#[test]
fn the_same_file_opened_twice_shares_its_lock() {
    let d = dir("twice");
    let p = saved_cad(&d, "part.cad");
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    let r = session.execute("open", &json!({ "path": p })).unwrap();
    assert!(r.get("readOnly").is_none(), "{r}");
    session.execute("close", &json!({})).unwrap();
    assert!(d.join("part.cdl").exists(), "the other drawing still has it open");
    session.execute("close", &json!({})).unwrap();
    assert!(!d.join("part.cdl").exists());
}

#[test]
fn a_lock_taken_over_or_removed_is_noticed_at_the_heartbeat() {
    let d = dir("heartbeat");
    let p = saved_cad(&d, "part.cad");
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    assert!(lock::refresh_locks(&mut session).is_some_and(|w| w <= lock::HEARTBEAT));
    // Removed (someone tidied the folder): taken again.
    std::fs::remove_file(d.join("part.cdl")).unwrap();
    lock::refresh_locks_at(&mut session, true);
    assert!(lock_text(&p).contains("app=CADCraft"));
    assert!(session.state().unwrap().lock.holder().is_none());
    // Taken over (this copy stopped refreshing it): read-only from then on.
    theirs(&p, 0);
    lock::refresh_locks_at(&mut session, true);
    assert_eq!(session.state().unwrap().lock.holder().map(|h| h.user.as_str()), Some("alex"));
    assert!(session.log.iter().any(|l| l.contains("taken over")), "{:?}", session.log);
    assert!(session.execute("qsave", &json!({})).is_err());
}

#[test]
fn closing_the_app_releases_every_lock() {
    let d = dir("exit");
    let a = saved_cad(&d, "a.cad");
    let b = saved_cad(&d, "b.cad");
    let mut session = Session::new();
    session.execute("open", &json!({ "path": a })).unwrap();
    session.execute("open", &json!({ "path": b })).unwrap();
    lock::release_locks(&mut session);
    assert!(!d.join("a.cdl").exists() && !d.join("b.cdl").exists());
}

#[test]
fn saving_checks_the_lock_even_without_a_heartbeat() {
    let d = dir("noheart");
    let p = saved_cad(&d, "part.cad");
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    let before = std::fs::read(&p).unwrap();
    // A script never calls the heartbeat; meanwhile someone took the lapsed lock.
    theirs(&p, 0);
    session.cmdline("line 0,0 1,1").unwrap();
    session.cmdline("").unwrap();
    let e = session.execute("qsave", &json!({})).unwrap_err().to_string();
    assert!(e.contains("taken over") && e.contains("alex"), "{e}");
    assert_eq!(std::fs::read(&p).unwrap(), before);
    // The lock was only removed (nobody else has it): saving takes it again.
    std::fs::remove_file(d.join("part.cdl")).unwrap();
    let mut session = Session::new();
    session.execute("open", &json!({ "path": p })).unwrap();
    std::fs::remove_file(d.join("part.cdl")).unwrap();
    session.execute("qsave", &json!({})).unwrap();
    assert!(lock_text(&p).contains("app=CADCraft"));
}
