//! The credential file, which is the part that decides who an agent is.
//!
//! Every refusal here is a refusal on purpose. A portal config that loaded anyway and left the
//! password empty would let an agent into the room as nobody, and a room where a message can
//! arrive with no name is a room nobody can be held to.

use super::*;

fn write(dir: &Path, text: &str) {
    std::fs::write(config_path(dir), text).unwrap();
}

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn a_good_config_loads() {
    let home = temp();
    write(
        home.path(),
        r#"{"api":"https://me-portal.example.workers.dev","password":"carls-own"}"#,
    );
    let config = Config::load(home.path()).unwrap();
    assert_eq!(config.api, "https://me-portal.example.workers.dev");
    assert_eq!(config.password, "carls-own");
    // Nobody has read anything yet, so the watermark starts at the beginning of the room
    // rather than at the end. A new machine should see what it missed.
    assert_eq!(config.seen, 0);
}

#[test]
fn a_missing_file_says_how_to_write_one() {
    let home = temp();
    let err = Config::load(home.path()).unwrap_err().to_string();
    // The message has to carry the shape of the file. Somebody hitting this has nothing else
    // to go on, and "no such file" would send them looking for a file they have never seen.
    assert!(err.contains("portal.json"), "{err}");
    assert!(err.contains("\"api\""), "{err}");
    assert!(err.contains("\"password\""), "{err}");
}

#[test]
fn plain_http_is_refused_rather_than_warned() {
    let home = temp();
    write(
        home.path(),
        r#"{"api":"http://me-portal.example.com","password":"carls-own"}"#,
    );
    let err = Config::load(home.path()).unwrap_err().to_string();
    assert!(err.contains("https"), "{err}");
    // The password is an identity in a room with JJ's mentor in it. Sending it in the clear
    // is not something to carry on after mentioning.
}

#[test]
fn an_empty_password_or_address_is_refused() {
    let home = temp();
    write(
        home.path(),
        r#"{"api":"https://x.workers.dev","password":"   "}"#,
    );
    assert!(Config::load(home.path()).is_err());

    write(home.path(), r#"{"api":"","password":"carls-own"}"#);
    assert!(Config::load(home.path()).is_err());
}

#[test]
fn rubbish_says_it_is_rubbish_rather_than_that_the_file_is_missing() {
    let home = temp();
    write(home.path(), "this is not json");
    let err = Config::load(home.path()).unwrap_err().to_string();
    assert!(err.contains("not valid JSON"), "{err}");
}

#[test]
fn remembering_the_watermark_leaves_the_password_alone() {
    let home = temp();
    write(
        home.path(),
        r#"{"api":"https://me-portal.example.workers.dev","password":"carls-own"}"#,
    );
    let config = Config::load(home.path()).unwrap();
    config.remember_seen(home.path(), 42).unwrap();

    let back = Config::load(home.path()).unwrap();
    assert_eq!(back.seen, 42);
    // The one thing in this file that cannot be recovered from anywhere else. A bug in the
    // watermark write must not be able to take it out.
    assert_eq!(back.password, "carls-own");
    assert_eq!(back.api, "https://me-portal.example.workers.dev");
}

#[test]
fn the_watermark_is_written_from_disk_rather_than_from_memory() {
    // Two agents share the room and one may have moved the watermark since this one loaded.
    // Writing from a stale in memory copy would move it backwards and replay the room.
    let home = temp();
    write(
        home.path(),
        r#"{"api":"https://x.workers.dev","password":"p","seen":5}"#,
    );
    let stale = Config::load(home.path()).unwrap();
    write(
        home.path(),
        r#"{"api":"https://x.workers.dev","password":"p","seen":90}"#,
    );
    stale.remember_seen(home.path(), 91).unwrap();
    assert_eq!(Config::load(home.path()).unwrap().seen, 91);
}

#[test]
fn the_credentials_live_outside_any_repository() {
    // Named here so that moving it into the tree fails a test rather than being noticed by
    // whoever reads the commit.
    let path = config_path(Path::new("/home/someone/.carl"));
    assert_eq!(path, Path::new("/home/someone/.carl/portal.json"));
}

#[test]
fn loopback_over_plain_http_is_allowed_because_nothing_leaves_the_machine() {
    let home = temp();
    for local in [
        "http://localhost:8787",
        "http://127.0.0.1:8787",
        "http://localhost",
    ] {
        write(
            home.path(),
            &format!(r#"{{"api":"{local}","password":"carls-own"}}"#),
        );
        assert!(
            Config::load(home.path()).is_ok(),
            "{local} should be allowed"
        );
    }
}

#[test]
fn a_host_that_merely_contains_the_word_localhost_is_still_refused() {
    // The mistake a substring check would make. This one is somebody else's machine on the
    // open internet, and handing it the password is exactly what the https rule prevents.
    let home = temp();
    for elsewhere in [
        "http://localhost.evil.example",
        "http://notlocalhost",
        "http://evil.example/?x=localhost",
        "http://127.0.0.1.evil.example",
    ] {
        write(
            home.path(),
            &format!(r#"{{"api":"{elsewhere}","password":"carls-own"}}"#),
        );
        assert!(
            Config::load(home.path()).is_err(),
            "{elsewhere} should be refused"
        );
    }
}
