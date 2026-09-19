//! A recorder that exits or stalls must not leave the service looking healthy forever.
use crate::{Error, Result};
use std::time::{Duration, Instant};

pub(super) fn for_audio(
    want: u64,
    timeout: Duration,
    mut progress: impl FnMut() -> (u64, bool),
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let (received, ended) = progress();
        if ended {
            return Err(Error::Refused(
                "microphone recorder stopped. Check the audio device and arecord output".into(),
            ));
        }
        if received >= want {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Refused(
                "microphone produced no new audio before the deadline".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn bounded_result(ended: bool) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(for_audio(32_000, Duration::from_millis(20), || (0, ended)));
        });
        rx.recv_timeout(Duration::from_secs(1))
            .expect("waiting for a dead recorder hung")
    }

    #[test]
    fn exited_recorder_is_reported_instead_of_hanging_calibration() {
        assert!(
            bounded_result(true)
                .unwrap_err()
                .to_string()
                .contains("stopped")
        );
    }

    #[test]
    fn stalled_recorder_is_reported_before_the_service_hangs() {
        assert!(
            bounded_result(false)
                .unwrap_err()
                .to_string()
                .contains("deadline")
        );
    }

    #[test]
    fn arriving_audio_completes_the_wait() {
        for_audio(32_000, Duration::from_millis(20), || (32_000, false)).unwrap();
    }
}
