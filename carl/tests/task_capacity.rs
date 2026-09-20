use carl::army::{Status, Task, Verification, may_take_on};

#[test]
fn unfinished_started_tasks_keep_the_workers_capacity() {
    let mut task = Task::assign(
        "mason",
        "nora",
        "repair",
        Verification::of(["checked"]).unwrap(),
    )
    .unwrap();
    for status in [
        Status::InHand,
        Status::Submitted,
        Status::ChangesRequested,
        Status::Blocked,
    ] {
        task.status = status;
        assert!(
            may_take_on("nora", std::slice::from_ref(&task)).is_err(),
            "{status:?}"
        );
        assert!(may_take_on("olivia", std::slice::from_ref(&task)).is_ok());
    }
    for status in [Status::Assigned, Status::Accepted, Status::Abandoned] {
        task.status = status;
        assert!(may_take_on("nora", std::slice::from_ref(&task)).is_ok());
    }
}
