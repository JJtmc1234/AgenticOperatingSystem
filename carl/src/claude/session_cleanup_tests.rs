use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn completed_handoff_does_not_wait_for_inherited_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fixture");
    std::fs::write(&program, "#!/usr/bin/python3\nimport subprocess,json,sys\nsys.stdin.readline()\nsubprocess.Popen(['/bin/sleep','4'])\nprint(json.dumps({'type':'result','result':'actual worker result'}),flush=True)\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut session = Runner::at(program)
        .open_session(&SessionId::fresh().unwrap(), dir.path(), "", false)
        .unwrap();
    let answer = session
        .ask("status", &mut |_| Flow::Continue, &mut || Flow::Continue)
        .unwrap();
    assert_eq!(answer.text, "actual worker result");
    let began = std::time::Instant::now();
    drop(session);
    assert!(
        began.elapsed() < std::time::Duration::from_secs(3),
        "completed answer trapped by inherited stdout"
    );
}

#[test]
fn completed_session_has_a_deadline_when_child_ignores_eof() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fixture");
    std::fs::write(&program, "#!/usr/bin/python3\nimport json,sys,time\nsys.stdin.readline()\nprint(json.dumps({'type':'result','result':'done'}),flush=True)\ntime.sleep(10)\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut session = Runner::at(program)
        .open_session(&SessionId::fresh().unwrap(), dir.path(), "", false)
        .unwrap();
    assert_eq!(
        session
            .ask("status", &mut |_| Flow::Continue, &mut || Flow::Continue)
            .unwrap()
            .text,
        "done"
    );
    let began = std::time::Instant::now();
    drop(session);
    assert!(began.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn panel_turn_returns_final_answer_without_waiting_for_stdout_eof() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("fixture");
    std::fs::write(&program, "#!/usr/bin/python3\nimport subprocess,json,sys\nsys.stdin.read()\nsubprocess.Popen(['/bin/sleep','4'])\nprint(json.dumps({'type':'result','result':'real final envelope'}),flush=True)\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    let id = SessionId::fresh().unwrap();
    let turn = super::super::Turn {
        session: &id,
        resume: false,
        prompt: "status",
        extra_system: None,
        workdir: dir.path(),
    };
    let began = std::time::Instant::now();
    let answer = Runner::at(program)
        .ask_streaming(&turn, &mut |_| Flow::Continue)
        .unwrap();
    assert_eq!(answer.text, "real final envelope");
    assert!(began.elapsed() < std::time::Duration::from_secs(3));
}
