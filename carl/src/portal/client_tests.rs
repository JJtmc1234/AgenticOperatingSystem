//! The client, against a real socket.
//!
//! A fake room rather than a mocked one. The thing worth checking here is that the two calls
//! put the password where the server looks for it and read back what the server actually
//! writes, and a mock that returns a `Said` without ever forming a request proves none of that.
//! Twenty lines of `TcpListener` is cheaper than the dependency and closer to the truth.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use super::*;

/// What the fake room saw, so a test can assert on the request rather than only the answer.
struct Seen {
    method: String,
    path: String,
    authorization: String,
    body: String,
}

/// Serves exactly one request, then hands back what it saw.
///
/// One request because every test here makes one call. A server that stayed up would need
/// shutting down, and a test that leaks a thread per case is a test suite that gets slower for
/// reasons nobody can see.
fn room_answering(status: u16, body: &'static str) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);

        let mut start = String::new();
        reader.read_line(&mut start).unwrap();
        let mut parts = start.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();

        let mut authorization = String::new();
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(v) = lower.strip_prefix("authorization:") {
                authorization = v.trim().to_string();
            }
            if let Some(v) = lower.strip_prefix("content-length:") {
                length = v.trim().parse().unwrap_or(0);
            }
        }

        let mut body_in = vec![0u8; length];
        if length > 0 {
            reader.read_exact(&mut body_in).unwrap();
        }
        let _ = tx.send(Seen {
            method,
            path,
            authorization,
            body: String::from_utf8_lossy(&body_in).to_string(),
        });

        let reason = if status == 200 { "OK" } else { "NO" };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: \
             {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = reader.into_inner();
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });

    (api, rx)
}

#[test]
fn saying_something_sends_the_password_and_returns_what_the_room_recorded() {
    let (api, seen) = room_answering(
        200,
        r#"{"id":7,"who":"Carl","text":"the build is green","at":1788000000}"#,
    );
    let said = Portal::new(&api, "carls-own")
        .say("the build is green")
        .unwrap();

    let seen = seen.recv().unwrap();
    assert_eq!(seen.method, "POST");
    assert!(seen.path.ends_with("/say"), "{}", seen.path);
    // In the header rather than the body. A body is what gets logged when somebody debugs a
    // request, and this one is an identity.
    assert_eq!(seen.authorization, "bearer carls-own");
    assert!(
        !seen.body.contains("carls-own"),
        "the password reached the body: {}",
        seen.body
    );
    assert!(seen.body.contains("the build is green"), "{}", seen.body);

    // The name came back from the server. Nothing in the request asked to be called Carl.
    assert!(
        !seen.body.contains("Carl"),
        "the client named itself: {}",
        seen.body
    );
    assert_eq!(said.who, "Carl");
    assert_eq!(said.id, 7);
}

#[test]
fn reading_asks_from_the_watermark_and_parses_the_room() {
    let (api, seen) = room_answering(
        200,
        r#"[{"id":8,"who":"JJ","text":"morning","at":1788000001},
            {"id":9,"who":"Hunter","text":"morning","at":1788000002}]"#,
    );
    let room = Portal::new(&api, "carls-own").read(7).unwrap();

    let seen = seen.recv().unwrap();
    assert_eq!(seen.method, "GET");
    assert!(seen.path.contains("after=7"), "{}", seen.path);
    assert_eq!(room.len(), 2);
    assert_eq!(room[0].who, "JJ");
    assert_eq!(room[1].id, 9);
}

#[test]
fn a_quiet_room_is_an_empty_list_rather_than_an_error() {
    let (api, _seen) = room_answering(200, "[]");
    assert_eq!(Portal::new(&api, "p").read(99).unwrap().len(), 0);
}

#[test]
fn a_rejected_password_says_that_rather_than_the_status_code() {
    let (api, _seen) = room_answering(401, r#"{"error":"no"}"#);
    let err = Portal::new(&api, "wrong").read(0).unwrap_err().to_string();
    // "http status 401" sends somebody looking at the network. The problem is one line in one
    // file, and the message has to say which.
    assert!(err.contains("portal.json"), "{err}");
    assert!(err.contains("401"), "{err}");
}

#[test]
fn a_missing_room_names_the_setting_that_is_wrong() {
    let (api, _seen) = room_answering(404, r#"{"error":"no such room"}"#);
    let err = Portal::new(&api, "p").say("hello").unwrap_err().to_string();
    assert!(err.contains("api"), "{err}");
}

#[test]
fn an_empty_message_never_reaches_the_room() {
    // No server at all. If this formed a request it would fail to connect and the error would
    // say so, so a refusal that names the message is proof it stopped before the socket.
    let err = Portal::new("https://127.0.0.1:1", "p")
        .say("   ")
        .unwrap_err()
        .to_string();
    assert!(err.contains("nothing to say"), "{err}");
}

#[test]
fn a_room_that_cannot_be_reached_says_so_plainly() {
    // Port 1 on loopback, which nothing is listening on.
    let err = Portal::new("http://127.0.0.1:1", "p")
        .read(0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("could not reach the room"), "{err}");
}

#[test]
fn a_trailing_slash_on_the_address_does_not_double_up() {
    let (api, seen) = room_answering(200, "[]");
    Portal::new(&format!("{api}/"), "p").read(0).unwrap();
    let seen = seen.recv().unwrap();
    assert!(!seen.path.contains("//read"), "{}", seen.path);
}
