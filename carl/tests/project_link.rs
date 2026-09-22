//! Task history remains readable after the Projects feature is retired.
use carl::army::event::{Event, Journal, read};
use carl::army::task::{Status, Task, Verification};
use carl::panel::tasks;

fn verification() -> Verification {
    Verification::of(["cargo test passes"]).unwrap()
}
fn delegate(journal: &mut Journal, task: &Task) {
    journal
        .append(
            &task.created_by,
            Event::Delegated {
                task: task.id.clone(),
                to: task.owner.clone(),
                goal: task.goal.clone(),
                parent: task.parent.clone(),
                must: task.verification.must.clone(),
                workspace: task.workspace.clone(),
                objective: task.objective,
            },
        )
        .unwrap();
}

const BEFORE_PROJECTS: &str = concat!(
    r#"{"seq":1,"at":1755200000,"actor":"mason","event":"delegated","#,
    r#""task":"a1b2c3","to":"nora","goal":"cache the prototype lookup"}"#,
);

/// Older still, from before the parent and verification conditions were carried either.
const BEFORE_ANYTHING: &str = concat!(
    r#"{"seq":2,"at":1755200001,"actor":"nora","event":"moved","#,
    r#""task":"a1b2c3","from":"assigned","to":"in hand"}"#,
);

#[test]
fn a_journal_written_before_projects_existed_still_opens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let old = format!("{BEFORE_PROJECTS}\n{BEFORE_ANYTHING}\n");
    std::fs::write(&path, &old).unwrap();
    let records = read(&path).unwrap();
    assert_eq!(records.len(), 2);
    match &records[0].event {
        Event::Delegated {
            parent, must, goal, ..
        } => {
            assert_eq!(*parent, None);
            assert!(must.is_empty());
            assert_eq!(goal, "cache the prototype lookup");
        }
        other => panic!("wrong event: {other:?}"),
    }
    let rebuilt = &tasks::fold(&records)[0];
    assert_eq!(rebuilt.status, "in hand");
    assert_eq!(rebuilt.owner, "nora");
    assert_eq!(std::fs::read_to_string(path).unwrap(), old);
}

#[test]
fn old_and_new_lines_live_in_one_journal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    std::fs::write(&path, format!("{BEFORE_PROJECTS}\n")).unwrap();
    let mut journal = Journal::open(&path).unwrap();
    let task = Task::assign("mason", "nora", "new assignment", verification()).unwrap();
    delegate(&mut journal, &task);
    let records = read(&path).unwrap();
    let folded = tasks::fold(&records);
    assert_eq!(folded.len(), 2);
    assert_eq!(folded[0].id, "a1b2c3");
    assert_eq!(folded[1].id, task.id.to_string());
    assert_eq!(folded[1].must, task.verification.must);
    assert_eq!(folded[1].assigner, "mason");
    assert_eq!(records[1].actor, "mason");
}

#[test]
fn split_task_parentage_and_workspace_remain_independent() {
    let parent = Task::assign("carl", "mason", "Factorio work", verification())
        .unwrap()
        .in_workspace("/tmp/parent");
    let child = Task::split_from(&parent, "mason", "nora", "part of it", verification())
        .unwrap()
        .in_workspace("/tmp/child");
    assert_eq!(child.parent, Some(parent.id));
    assert_eq!(child.owner, "nora");
    assert_eq!(parent.workspace.as_deref(), Some("/tmp/parent"));
    assert_eq!(child.workspace.as_deref(), Some("/tmp/child"));
}

#[test]
fn workers_still_cannot_create_or_accept_their_own_work() {
    assert!(Task::assign("nora", "nora", "mine now", verification()).is_err());
    let mut task = Task::assign("mason", "nora", "cache lookup", verification()).unwrap();
    task.advance("nora", Status::InHand).unwrap();
    assert!(task.advance("nora", Status::Accepted).is_err());
    assert_eq!(task.status, Status::InHand);
}
