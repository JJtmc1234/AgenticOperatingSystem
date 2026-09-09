//! Direct CLI conversations use their own sessions and keep permission hooks.
use crate::claude::{Answer, Flow, Runner, Say, Turn};
use crate::{Log, Registry, Result, Speaker, ThreadId};
use std::path::Path;

pub fn stream(
    home: &Path,
    text: &str,
    cwd: &str,
    model: &str,
    on_text: &mut dyn FnMut(Say<'_>) -> Flow,
) -> Result<Answer> {
    let mut runner = Runner::default().as_native_cli().asking_jj(home, "code");
    if !model.trim().is_empty() {
        runner = runner.running(model.trim());
    }
    run(home, text, cwd, &runner, on_text)
}

fn run(
    home: &Path,
    text: &str,
    cwd: &str,
    runner: &Runner,
    on_text: &mut dyn FnMut(Say<'_>) -> Flow,
) -> Result<Answer> {
    let directory = if cwd.trim().is_empty() {
        home.join("workspace")
    } else {
        cwd.into()
    };
    let directory = directory.canonicalize()?;
    if !directory.is_dir() {
        return Err(crate::Error::Refused("Choose a project directory".into()));
    }
    // Encode the canonical path in directories rather than hashing identities.
    // Each component stays below filesystem filename limits.
    use std::os::unix::ffi::OsStrExt;
    let encoded: String = directory
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut sessions = home.join("code-sessions");
    for chunk in encoded.as_bytes().chunks(64) {
        sessions.push(std::str::from_utf8(chunk).expect("hex is ASCII"));
    }
    std::fs::create_dir_all(&sessions)?;
    let thread = ThreadId::new("panel-code")?;
    let mut log = Log::open(home.join("conversations.jsonl"))?;
    log.append(
        super::exchange::now(),
        thread.clone(),
        Speaker::Human,
        text,
        Some(crate::brief::OWNER.into()),
    )?;
    let mut registry = Registry::open(sessions.join("threads.json"))?;
    let (session, is_new) = registry.session_for(&thread, super::exchange::now())?;
    let answer = runner.ask_streaming(
        &Turn {
            session: &session,
            resume: !is_new,
            prompt: text,
            extra_system: None,
            workdir: &directory,
        },
        on_text,
    );
    match answer {
        Ok(answer) => {
            log.append(
                super::exchange::now(),
                thread.clone(),
                Speaker::Carl,
                &answer.text,
                None,
            )?;
            if answer.interrupted {
                log.append(
                    super::exchange::now(),
                    thread.clone(),
                    Speaker::System,
                    "CLI turn interrupted",
                    None,
                )?;
            }
            registry.record_turn(&thread)?;
            Ok(answer)
        }
        Err(error) => {
            log.append(
                super::exchange::now(),
                thread,
                Speaker::System,
                format!("no CLI answer: {error}"),
                None,
            )?;
            Err(error)
        }
    }
}
