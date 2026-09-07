use super::*;

#[test]
fn iris_cli_preserves_request_and_workflow_flags() {
    let request = "check ' spaced $(request)";
    let cli = Cli::try_parse_from([
        "carl",
        "--home",
        "/tmp/carl home",
        "iris",
        "run",
        "--request",
        request,
    ])
    .unwrap();
    assert_eq!(cli.home, "/tmp/carl home");
    match cli.command {
        Command::Iris { args } => assert_eq!(
            args,
            ["run", "--request", request].map(std::ffi::OsString::from)
        ),
        _ => panic!("expected Iris command"),
    }
}
