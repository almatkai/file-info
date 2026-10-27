//! These tests create only generated owned paths inside this package's ignored QA.
#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
};
use tezbar_file_info::secure::Selected;
fn dir() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".qa/rust-files")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn rejects_special_links_and_readonly() {
    let d = dir();
    let p = d.join("owned.txt");
    fs::write(&p, b"generated").unwrap();
    let link = d.join("link");
    symlink(&p, &link).unwrap();
    assert!(Selected::open(link.to_str().unwrap(), true).is_err());
    let hard = d.join("hard");
    fs::hard_link(&p, &hard).unwrap();
    assert!(Selected::open(p.to_str().unwrap(), true).is_err());
    let ro = d.join("readonly");
    fs::write(&ro, b"generated").unwrap();
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o444)).unwrap();
    assert!(Selected::open(ro.to_str().unwrap(), true).is_err());
    assert!(Selected::open(d.to_str().unwrap(), true).is_err());
}
#[test]
fn rejects_symlink_ancestor_and_changed_identity() {
    let d = dir();
    let sub = d.join("sub");
    fs::create_dir(&sub).unwrap();
    let p = sub.join("owned\nфайл ; $().txt");
    fs::write(&p, b"generated").unwrap();
    let alias = d.join("alias");
    symlink(&sub, &alias).unwrap();
    assert!(Selected::open(alias.join(p.file_name().unwrap()).to_str().unwrap(), false).is_err());
    let selected = Selected::open(p.to_str().unwrap(), true).unwrap();
    fs::rename(&p, sub.join("original-retained")).unwrap();
    fs::write(&p, b"unrelated generated file").unwrap();
    assert!(selected.recheck().is_err());
    assert_eq!(fs::read(&p).unwrap(), b"unrelated generated file");
}
#[test]
fn destination_stage_commit_is_fenced_and_reports_real_os_attrs() {
    let d = dir();
    let p = d.join("owned.txt");
    fs::write(&p, b"generated").unwrap();
    let selected = Selected::open(p.to_str().unwrap(), true).unwrap();
    let mut stage = selected.stage("txt", b"replacement").unwrap();
    let outcome = selected.commit(&mut stage).unwrap();
    assert!(outcome.detail.contains("committed"));
    assert_eq!(fs::read(&p).unwrap(), b"replacement");
}
#[tokio::test]
async fn directory_info_is_readonly_and_clear_stays_rejected() {
    let d = dir();
    for nonempty in [false, true] {
        let selected_dir = d.join(if nonempty { "nonempty.png" } else { "empty" });
        fs::create_dir(&selected_dir).unwrap();
        let child = selected_dir.join("owned.txt");
        if nonempty {
            fs::write(&child, b"owned private child metadata").unwrap();
        }
        let selected = Selected::open(selected_dir.to_str().unwrap(), false).unwrap();
        let info = tezbar_file_info::info::get(&selected).await.unwrap();
        assert_eq!(info.file_type(), "Directory");
        assert_eq!(info.icon(), "Folder");
        assert!(info.copy_all().contains("Type: Directory"));
        assert!(info.dimensions.is_empty() && info.duration.is_empty());
        assert!(Selected::open(selected_dir.to_str().unwrap(), true).is_err());
        assert!(selected.stage("bin", b"never stage a directory").is_err());
        if nonempty {
            assert_eq!(fs::read(&child).unwrap(), b"owned private child metadata");
        }
    }
}
#[test]
fn validated_candidate_replacement_is_rejected() {
    let d = dir();
    let p = d.join("original.txt");
    fs::write(&p, b"frozen original").unwrap();
    let selected = Selected::open(p.to_str().unwrap(), true).unwrap();
    let mut stage = selected.stage("txt", b"native candidate").unwrap();
    stage.reopen().unwrap();
    // Locate only this test's owned private directory, never scan user paths.
    let private = fs::read_dir(&d)
        .unwrap()
        .map(|p| p.unwrap().path())
        .find(|p| p.is_dir())
        .unwrap();
    fs::rename(
        private.join(&stage.leaf),
        private.join("retained-candidate"),
    )
    .unwrap();
    fs::write(private.join(&stage.leaf), b"unexpected replacement").unwrap();
    assert!(selected.commit(&mut stage).is_err());
    assert_eq!(fs::read(&p).unwrap(), b"frozen original");
}
