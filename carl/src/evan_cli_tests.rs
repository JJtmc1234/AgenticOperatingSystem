use super::*;

#[test]
fn evan_cli_preserves_request_and_workflow_flags() {
    let request = "check ' spaced $(request)";
    let cli = Cli::try_parse_from([
        "carl",
        "--home",
        "/tmp/carl home",
        "evan",
        "run",
        "--repo",
        request,
    ])
    .unwrap();
    assert_eq!(cli.home, "/tmp/carl home");
    match cli.command {
        Command::Evan { args } => assert_eq!(
            args,
            ["run", "--repo", request].map(std::ffi::OsString::from)
        ),
        _ => panic!("expected Evan command"),
    }
}
