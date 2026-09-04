//! The real `carl panel` binary as a child process, shared by every integration test that needs
//! one.
//!
//! A thread inside the test process was tried first and was wrong in a way worth writing down:
//! unlinking a socket does not break connections that are already open, so a subscribed client
//! carried on being served by the old thread and never noticed anything. Nothing about reconnect
//! was being tested at all.
//!
//! A real child process fixes that, because killing it closes every connection it holds. It also
//! means these tests exercise the binary JJ actually runs, including its signal handling.

// Compiled into each test target separately, and no single one uses every method here.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use carl::army::task::Verification;
use carl::panel::client::PanelClient;
use carl::panel::listen;

pub struct Backend {
    home: PathBuf,
    child: Option<std::process::Child>,
}

impl Backend {
    pub fn start(home: &Path) -> Self {
        let mut me = Self {
            home: home.to_path_buf(),
            child: None,
        };
        me.up();
        me
    }

    /// Starts the real binary, and returns only once its socket is answering.
    ///
    /// Waiting for a real connection rather than sleeping a guessed interval, so nothing here
    /// races the process it just spawned.
    pub fn up(&mut self) {
        // Cargo substitutes this path when the test is compiled, and its fingerprint does not
        // include it. So moving the repository leaves a cached test binary pointing at the old
        // directory, and the only symptom is a bare "No such file or directory" from spawn with
        // no path in it. Naming the path turns an hour of hunting into a rebuild.
        let binary = env!("CARGO_BIN_EXE_carl");
        assert!(
            Path::new(binary).exists(),
            "no carl binary at {binary}, which is the path baked in when this test was compiled. \
             The repository has moved since. Run `touch tests/common/mod.rs` and build again."
        );

        let child = std::process::Command::new(binary)
            .arg("--home")
            .arg(&self.home)
            .arg("panel")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("starting {binary}: {e}"));
        self.child = Some(child);

        for _ in 0..400 {
            if PanelClient::connect(&self.socket()).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the backend never came up");
    }

    /// Stops it the way systemd would, and waits until it is really gone.
    pub fn down(&mut self) {
        self.kill();
        for _ in 0..400 {
            if PanelClient::connect(&self.socket()).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the backend never went away");
    }

    pub fn socket(&self) -> PathBuf {
        listen::socket_path(&self.home)
    }

    /// The child's pid, for a test that needs to send it a real signal rather than SIGKILL.
    pub fn pid(&self) -> u32 {
        self.child.as_ref().expect("the backend is running").id()
    }

    /// Reaps a child that something other than `down` has already told to stop.
    pub fn reap(&mut self) {
        self.child
            .as_mut()
            .expect("the backend is running")
            .wait()
            .unwrap();
        self.child = None;
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.kill();
    }
}

/// The one verification these tests use. None of them are about what makes a good one.
pub fn verification() -> Verification {
    Verification::of(["cargo test passes"]).unwrap()
}
