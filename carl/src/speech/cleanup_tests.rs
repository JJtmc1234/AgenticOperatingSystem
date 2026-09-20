use super::*;

#[test]
fn dropping_finished_playback_reaps_piper_even_when_it_is_still_running() {
    for slow in [false, true] {
        let piper = if slow {
            Command::new("/bin/sleep").arg("30").spawn().unwrap()
        } else {
            Command::new("/bin/true").spawn().unwrap()
        };
        let pid = piper.id() as libc::pid_t;
        let mut speaking = Speaking {
            piper,
            player: Command::new("/bin/true").spawn().unwrap(),
            stdin: None,
            said_anything: true,
        };
        speaking.player.wait().unwrap();
        assert!(speaking.done());
        drop(speaking);
        let mut status = 0;
        // This PID belongs to our child. An unreaped child cannot have its PID reused.
        let reaped = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        let error = std::io::Error::last_os_error();
        if reaped == 0 {
            // Clean up the still owned child before reporting the regression failure.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
                libc::waitpid(pid, &mut status, 0);
            }
        }
        assert_eq!(reaped, -1, "piper was still an unreaped child");
        assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
    }
}
