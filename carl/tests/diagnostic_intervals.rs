use carl::providers::system::proc::parse_cpu_times;

#[test]
fn guest_cpu_time_is_counted_once_in_total_and_busy_fraction() {
    let before = parse_cpu_times("cpu 100 20 30 400 50 0 0 0 80 10").unwrap();
    let after = parse_cpu_times("cpu 150 30 30 440 50 0 0 0 120 20").unwrap();
    assert_eq!(before.total, 600);
    assert_eq!(after.total, 700);
    assert!((after.busy_fraction_since(&before).unwrap() - 0.6).abs() < 1e-9);
}

use carl::army::event::{Event, Record};
use carl::army::{Status, TaskId};
use carl::providers::army::journal::fold;

fn record(at: u64, event: Event) -> Record {
    Record {
        seq: at + 1,
        at,
        actor: "mason".into(),
        event,
    }
}

#[test]
fn handover_latency_ends_at_first_submission_not_review_or_resubmission() {
    let task = TaskId::quoted("latency-task");
    let events = vec![
        record(
            0,
            Event::Delegated {
                task: task.clone(),
                to: "nora".into(),
                goal: "repair".into(),
                parent: None,
                must: vec![],
                project: None,
                workspace: None,
                objective: None,
            },
        ),
        record(
            100,
            Event::Submitted {
                task: task.clone(),
                attempt: 1,
                words: 10,
            },
        ),
        record(
            4000,
            Event::Reviewed {
                task: task.clone(),
                accepted: false,
                why: "change it".into(),
            },
        ),
        record(
            5000,
            Event::Submitted {
                task: task.clone(),
                attempt: 2,
                words: 15,
            },
        ),
        record(
            6000,
            Event::Reviewed {
                task: task.clone(),
                accepted: true,
                why: "accepted".into(),
            },
        ),
        record(
            7000,
            Event::moved(&task, Status::Submitted, Status::Accepted),
        ),
    ];
    let folded = fold(&events);
    assert_eq!(folded.handback_seconds(), vec![100]);
    assert_eq!(folded.tasks[0].elapsed(), 7000);
}

#[test]
fn a_submission_without_a_recorded_delegation_has_no_measured_latency() {
    let task = TaskId::quoted("orphan-submission");
    let folded = fold(&[record(
        100,
        Event::Submitted {
            task,
            attempt: 1,
            words: 10,
        },
    )]);
    assert!(folded.handback_seconds().is_empty());
}
