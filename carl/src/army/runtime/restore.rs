//! Recover a completed delivery if the runtime cache write was interrupted.
use super::store::Roll;
use crate::Result;
use crate::army::event::{Event, Record};
use std::path::Path;

pub(super) fn established(home: &Path, roll: &mut Roll) -> Result<()> {
    let text = match std::fs::read_to_string(home.join("run/events.jsonl")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let events: std::result::Result<Vec<Record>, _> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect();
    // Incomplete or unreadable history cannot establish a conversation.
    let Ok(events) = events else {
        return Ok(());
    };
    let pending: Vec<_> = roll
        .all()
        .filter(|record| !record.established)
        .cloned()
        .collect();
    for mut record in pending {
        let latest = events
            .iter()
            .rev()
            .find(|event| event.event.agent() == Some(&record.agent));
        if let Some(Record {
            event: Event::AgentSessionEstablished { session, .. },
            ..
        }) = latest
            && record.session.as_ref() == Some(session)
        {
            record.established = true;
            roll.save(home, record)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SessionId,
        army::{event::Journal, personnel::AgentId, runtime::Runtime},
    };

    #[test]
    fn later_failures_other_sessions_and_unreadable_history_cannot_establish_a_session() {
        for scenario in ["failed", "other", "unreadable"] {
            let home = tempfile::tempdir().unwrap();
            let id = AgentId::fresh().unwrap();
            let session = SessionId::fresh().unwrap();
            let mut record = Runtime::never(id.clone(), "evan", 1);
            record.session = Some(session.clone());
            let mut roll = Roll::open(home.path()).unwrap();
            let path = home.path().join("run/events.jsonl");
            let mut journal = Journal::open(&path).unwrap();
            journal
                .append(
                    "supervisor",
                    Event::AgentSessionEstablished {
                        agent: id.clone(),
                        name: "evan".into(),
                        session,
                    },
                )
                .unwrap();
            match scenario {
                "failed" => {
                    journal
                        .append(
                            "supervisor",
                            Event::AgentCrashed {
                                agent: id.clone(),
                                name: "evan".into(),
                                code: Some(1),
                                attempt: 1,
                            },
                        )
                        .unwrap();
                }
                "other" => record.session = Some(SessionId::fresh().unwrap()),
                _ => {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .append(true)
                        .open(&path)
                        .unwrap()
                        .write_all(b"broken\n")
                        .unwrap();
                }
            }
            roll.save(home.path(), record).unwrap();
            established(home.path(), &mut roll).unwrap();
            assert!(!roll.get(&id).unwrap().established, "{scenario}");
        }
    }
}
