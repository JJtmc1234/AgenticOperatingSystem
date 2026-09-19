//! Select the actual cancelled node without depending on the Pulse ALSA plugin.
use std::process::Command;

pub(super) fn command(source: Option<&str>) -> Command {
    let mut cmd = Command::new("arecord");
    cmd.args([
        "--quiet",
        "--format",
        "S16_LE",
        "--rate",
        &super::RATE.to_string(),
        "--channels",
        "1",
        "--file-type",
        "raw",
    ]);
    if let Some(name) = source {
        cmd.args(["-D", &format!("pipewire:NODE={name}")]);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_microphone_uses_pipewire_without_the_pulse_plugin() {
        let cmd = command(Some("carl-mic"));
        let args: Vec<_> = cmd.get_args().collect();
        let at = args.iter().position(|a| *a == "-D").unwrap();
        assert_eq!(args[at + 1], "pipewire:NODE=carl-mic");
    }

    #[test]
    fn default_microphone_keeps_the_system_device() {
        assert!(!command(None).get_args().any(|a| a == "-D"));
    }
}
