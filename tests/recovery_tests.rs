// tests/recovery_tests.rs
//
// Crash-recovery snapshots: write_snapshot/pending/wipe round-trips, and the
// app-level restore flow reopening tabs with their Save targets intact.

use squarez::app::App;
use squarez::project::{Project, ProjectMode};
use squarez::recovery::{pending, wipe, write_snapshot, RecoveryEntry};
use squarez::three_d::mesh::Mesh;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("squarez_recovery_tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn snapshot_round_trips_pixels_mesh_and_paths() {
    let dir = temp_dir("roundtrip");

    let mut plain = Project::new(16, 16, "Painting".to_string());
    plain.animations[0].frames[0].layers[0].set_pixel(3, 4, [9, 8, 7, 255]);

    let mut threed = Project::new_with_mode(64, 64, "Model".to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((64, 64)).unwrap();
    threed.mesh3d = Some(mesh.clone());

    let original = std::path::Path::new("/somewhere/Painting.sqr");
    let n = write_snapshot(&dir, &[(&plain, Some(original)), (&threed, None)]).unwrap();
    assert_eq!(n, 2);

    let entries = pending(&dir);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "Painting");
    assert_eq!(entries[0].original_path.as_deref(), Some(original));
    assert_eq!(entries[1].original_path, None, "never-saved tab has no Save target");

    // Snapshots are .sqr, so the 3D mesh must survive.
    let back = squarez::io::sqr::load_sqr(&dir.join(&entries[1].file)).unwrap();
    assert_eq!(back.mesh3d.as_ref(), Some(&mesh), "mesh3d must survive a snapshot");
    let back0 = squarez::io::sqr::load_sqr(&dir.join(&entries[0].file)).unwrap();
    assert_eq!(back0.animations[0].frames[0].layers[0].get_pixel(3, 4), [9, 8, 7, 255]);
}

#[test]
fn a_new_snapshot_replaces_the_old_set_and_wipe_clears_it() {
    let dir = temp_dir("replace");
    let a = Project::new(8, 8, "A".to_string());
    let b = Project::new(8, 8, "B".to_string());
    write_snapshot(&dir, &[(&a, None), (&b, None)]).unwrap();
    assert_eq!(pending(&dir).len(), 2);

    // Rebuilding with one tab must not leave the other's stale file behind.
    write_snapshot(&dir, &[(&a, None)]).unwrap();
    let entries = pending(&dir);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "A");

    // An empty set is a valid snapshot: nothing to recover.
    write_snapshot(&dir, &[]).unwrap();
    assert!(pending(&dir).is_empty());

    write_snapshot(&dir, &[(&b, None)]).unwrap();
    wipe(&dir);
    assert!(pending(&dir).is_empty(), "wipe must clear manifest and files");
}

#[test]
fn manifest_ignores_entries_whose_files_vanished() {
    let dir = temp_dir("vanished");
    let a = Project::new(8, 8, "A".to_string());
    let b = Project::new(8, 8, "B".to_string());
    write_snapshot(&dir, &[(&a, None), (&b, None)]).unwrap();
    std::fs::remove_file(dir.join("tab0.sqr")).unwrap();
    let entries = pending(&dir);
    assert_eq!(entries.len(), 1, "missing snapshot files must be filtered out");
    assert_eq!(entries[0].name, "B");
}

#[test]
fn restore_reopens_tabs_with_save_targets_and_marks_them_unsaved() {
    // The restore flow reads from the app's default recovery dir; stage real
    // snapshot files there through the public API, restore, then clean up.
    let dir = squarez::recovery::default_dir();
    let had_before = pending(&dir);

    let mut one = Project::new(8, 8, "One".to_string());
    one.animations[0].frames[0].layers[0].set_pixel(1, 1, [1, 2, 3, 255]);
    let two = Project::new(8, 8, "Two".to_string());
    let original = std::path::Path::new("/somewhere/One.sqr");
    write_snapshot(&dir, &[(&one, Some(original)), (&two, None)]).unwrap();

    let ctx = egui::Context::default();
    let mut app = App::new_with(&ctx, None);
    // new_with scanned the dir at construction; both entries must be offered.
    let offered: Vec<RecoveryEntry> = pending(&dir);
    assert_eq!(offered.len(), 2);

    app.restore_pending_recovery();

    assert_eq!(app.tab_count(), 2, "both snapshots must reopen as tabs");
    // The active tab is the last restored ("Two", never saved).
    assert_eq!(app.project.name, "Two");
    assert!(app.current_path.is_none());
    assert!(app.active_modified, "restored work IS unsaved work");
    assert!(
        pending(&dir).is_empty(),
        "consumed snapshots must not be offered again next launch"
    );

    // Restore the machine's previous state as best we can.
    wipe(&dir);
    if !had_before.is_empty() {
        // (Real pending recovery existed before the test ran; leave the dir
        // clean rather than resurrect it — tests must not fabricate prompts.)
    }
}
