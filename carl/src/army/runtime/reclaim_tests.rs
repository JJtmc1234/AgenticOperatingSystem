use super::*;

struct OwnedOrphan(std::process::Child);

impl Drop for OwnedOrphan {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_tick_reclaims_orphans_without_duplicate_processes_or_lost_sessions() {
    for established in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let people = army(home.path());
        let nora = id_of(&people, "nora");
        let orphan = OwnedOrphan(
            std::process::Command::new(stays_up())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let pid = orphan.0.id();
        let started = crate::providers::system::started::started(pid).unwrap();
        let session = crate::SessionId::fresh().unwrap();
        let mut record = Runtime::never(nora.clone(), "nora", 1);
        record.lifecycle = Lifecycle::Running {
            pid,
            started,
            since: 1,
        };
        record.session = Some(session.clone());
        record.established = established;
        record.supervisor = Some(u32::MAX);
        let mut roll = Roll::open(home.path()).unwrap();
        roll.save(home.path(), record).unwrap();
        drop(roll);

        let mut sup = supervisor(home.path(), &stays_up());
        let tick = sup.tick(&people, 1000).unwrap();
        assert!(
            tick.what
                .iter()
                .any(|(name, outcome)| { name == "nora" && matches!(outcome, Outcome::Reclaimed) })
        );
        assert!(
            !crate::providers::system::started::is_still(pid, started),
            "the replaced orphan must not remain running"
        );
        let replacement = sup.roll().get(&nora).unwrap();
        let replacement_pid = replacement.lifecycle.pid().unwrap();
        assert_ne!(replacement_pid, pid);
        assert_eq!(replacement.agent, nora);
        if established {
            assert_eq!(replacement.session.as_ref(), Some(&session));
        } else {
            assert_ne!(replacement.session.as_ref(), Some(&session));
        }
        let again = sup.tick(&people, 1001).unwrap();
        assert_eq!(
            again.count(|outcome| matches!(outcome, Outcome::Left)),
            everybody()
        );
        assert_eq!(
            sup.roll().get(&nora).unwrap().lifecycle.pid(),
            Some(replacement_pid)
        );
    }
}
