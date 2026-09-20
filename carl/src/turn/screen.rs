use super::*;

pub(super) fn runner(home: &Path) -> Result<Runner> {
    runner_for(home, crate::claude::permits::Surface::Jj)
}

pub(super) fn exchange<'a>(
    home: &'a Path,
    thread: &'a ThreadId,
    question: &'a str,
    sent: &'a str,
) -> Exchange<'a> {
    Exchange {
        home,
        thread,
        said: question,
        sent: Some(sent),
        author: Some(crate::brief::OWNER.to_string()),
        memory_source: Some("Carl interpreting JJ's screenshot"),
        extra: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_notes_identify_an_inference_and_preserve_the_human_question_author() {
        let home = tempfile::tempdir().unwrap();
        let thread = ThreadId::new("look").unwrap();
        exchange(home.path(), &thread, "What do you see?", "Read screen.png")
            .run(|_| {
                Ok(Answer {
                    text: "I see a factory.\n[remember] The screenshot appears to show a factory"
                        .into(),
                    interrupted: false,
                    session_id: None,
                    cost_usd: None,
                })
            })
            .unwrap();
        let note = std::fs::read_to_string(home.path().join("memory").join(format!(
            "{}.md",
            crate::remember::note_name("The screenshot appears to show a factory")
        )))
        .unwrap();
        assert!(note.contains("Carl interpreting JJ's screenshot"), "{note}");
        assert!(
            !note.contains("(said by JJ)"),
            "inference must not become JJ's statement"
        );
        let log = crate::log::read(home.path().join("conversations.jsonl")).unwrap();
        let question = log
            .iter()
            .find(|e| e.speaker == crate::Speaker::Human)
            .unwrap();
        assert_eq!(question.author.as_deref(), Some(crate::brief::OWNER));
        assert_eq!(question.text, "What do you see?");
    }

    #[test]
    fn screenshot_runner_obeys_jjs_permission_mode_and_chief_scope() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("permissions.json"),
            r#"{"jj":{"mode":"acceptEdits","allow":["Write","Bash(python3:*)"]}}"#,
        )
        .unwrap();
        let session = crate::SessionId::fresh().unwrap();
        let args = runner(home.path()).unwrap().args_for(&Turn {
            session: &session,
            resume: false,
            prompt: "look",
            extra_system: None,
            workdir: home.path(),
        });
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--permission-mode" && w[1] == "acceptEdits"),
            "{args:?}"
        );
        for tool in ["Write", "Bash(python3:*)", "Bash(carl-python:*)"] {
            assert!(!args.iter().any(|arg| arg == tool), "{args:?}");
        }
    }
}
