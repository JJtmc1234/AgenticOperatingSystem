use super::*;
use carl::panel::wire::{Ask, Frame, Reply, Request};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::{Duration, Instant};

fn request(stream: &UnixStream) -> Request {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn done(stream: &mut UnixStream, id: String) {
    writeln!(
        stream,
        "{}",
        serde_json::to_string(&Frame::to(
            Some(id),
            Reply::Done {
                seq: None,
                what: "answered".into(),
            }
        ))
        .unwrap()
    )
    .unwrap();
}

#[test]
fn an_allow_reaches_the_backend_before_the_waiting_turn_finishes() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("panel.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (started, has_started) = channel();
    let backend = thread::spawn(move || {
        let (mut turn, _) = listener.accept().unwrap();
        let turn_request = request(&turn);
        assert!(matches!(turn_request.body, Ask::Command { .. }));
        started.send(()).unwrap();
        listener.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        let mut answered = false;
        while Instant::now() < until {
            if let Ok((mut answer, _)) = listener.accept() {
                let incoming = request(&answer);
                assert!(matches!(
                    incoming.body,
                    Ask::Answered {
                        verdict: carl::panel::permission::Verdict::Allow,
                        ..
                    }
                ));
                done(&mut answer, incoming.id);
                answered = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        done(&mut turn, turn_request.id);
        answered
    });
    let (orders, take_orders) = channel();
    let (results, _incoming) = channel();
    commander(socket, take_orders, results);
    orders.send(Command::SayToCarl("work".into())).unwrap();
    has_started.recv_timeout(Duration::from_secs(2)).unwrap();
    orders
        .send(Command::AnswerPermission {
            question: "pending".into(),
            allow: true,
        })
        .unwrap();
    assert!(
        backend.join().unwrap(),
        "Allow waited behind the turn that needed it"
    );
}

#[test]
fn answering_permission_does_not_finish_carls_turn() {
    let (mut source, tx, _orders) = LivePanelDataSource::detached(Snapshot::default());
    source.submit(Command::SayToCarl("work".into())).unwrap();
    source.poll();
    tx.send(FromBackend::PermissionAnswered(Ok(()))).unwrap();
    assert!(source.poll().is_empty());
    assert!(source.speaking);
    tx.send(FromBackend::Settled(Ok(()))).unwrap();
    assert!(source.poll().iter().any(|event| matches!(
        event,
        PanelEvent::CarlSaid {
            streaming: false,
            ..
        }
    )));
}
