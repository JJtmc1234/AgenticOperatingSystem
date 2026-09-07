# Iris verification

Verified on Ubuntu on 7 September 2026. Arch boot and authentication remain target machine checks.

## Automated checks

35 Python tests passed using real temporary Git repositories and mocked model and GitHub writes.
The installed Carl build passed 1399 Rust tests, with zero failures and one ignored test.
Rust formatting and Clippy passed for that build. Installer ShellCheck passed.
Regression tests were run against deliberately restored old behavior and failed before restoration.

## Live checks

GitHub authentication discovered 26 owned repositories and read AOS issues and comments.
A tool-free Claude invocation returned validated structured output.
The Adrian to Iris handoff ran `carl iris doctor` successfully and returned publication enabled.
Carl recorded that handoff at sequence 1115.

The initial source batch was too large for the call budget. After reducing it to eight files
and selecting medium reasoning effort, both investigators and independent review completed.
The successful manual run occupies Iris journal sequences 28 through 42 and inspected eight files
at AOS commit `20297ec61ae0717201d485d1fc0ce0820dc01221`. No findings were accepted and no issue
was created. Real GitHub issue creation is covered by mocked adapter tests, not claimed as a
live write in this run.

The timer is enabled and active. Its first scheduled run completed with service result `success`
and exit code 0. The next five-minute poll was confirmed by systemd.

## Coverage and budget

The manual scan completed 1 of 59 eligible source batches and excluded 92 noneligible files.
This proves the workflow for a batch, not a complete audit of AOS or every repository.
Remaining batches are queued. Investigations stop when the daily allowance cannot cover another
pair of investigators and reviewer. Validation runs reserved $4 of the $5 daily allowance.
These reservations are conservative limits and are not a statement of actual billed cost.

Use `carl iris status` for the current report and `systemctl --user list-timers aos-iris.timer`
for the next scheduled check. Reports and audit state are under `~/.carl/iris`.
