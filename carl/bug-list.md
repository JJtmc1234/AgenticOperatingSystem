# bug list

Every bug of consequence, and the test that stops it coming back. No entry without a test.

| n | bug | how it showed up | guard |
|---|---|---|---|
| 1 | The microphone consumer read 0.6s per pass while a pass cost more than that, so it fell behind real time forever. | "it doesn't pick up what I say" | rewritten with a reader thread, `the_ring_never_grows_past_its_window` |
| 2 | Silence was measured as peak against a fixed floor, and one key press puts peak at 0.24 in a quiet room, so a recording never ended early. | Carl woke and then never answered | `one_click_fools_peak_and_does_not_fool_rms` |
| 3 | Carl heard his own voice through the speakers and answered his own answer. | replies to himself | echo cancellation, and `Devices` falling back to muting |
| 4 | `say` wrote the whole text into piper before starting the player, so a long answer deadlocked both. | never fired, found by reading | the player starts before any text is written |
| 5 | Slack was sent a JSON body for `users.info`, which only reads form parameters, so the id was dropped and Slack reported `user_not_found`. | Carl could not work out anybody's name | `user_not_found_mentions_the_encoding_trap` |
| 6 | A bare name in a channel produced an empty question, which the CLI refuses. | Carl posted a raw CLI error into a channel, twice | `an_empty_question_is_refused_here_rather_than_by_the_cli` |
| 7 | Two echo cancellers ran at once, so naming a device picked an unpredictable half of an unpredictable pair and nothing was cancelled. | Carl answered himself in a loop while reporting echo cancelled audio | `two_cancellers_are_refused_rather_than_gambled_on` |

| 8 | Overhaul mod families were matched with spaces removed from the mod name but not from the pattern, so any pattern containing a space could never match. | Space Exploration was installed and silently never reported | `no_overhaul_stem_can_be_impossible_to_match` |

| 9 | Mods were read from the directory rather than from `mod-list.json`, so mods present but switched off were reported as active. | 88 mods on disk and 4 enabled, so Carl was told a vanilla Space Age save was Sea Block with Angel's and Bob's, and answered a smelting question with ore processing that does not exist in that game | `only_the_mods_that_are_switched_on_are_reported` |

| 10 | `eframe` was declared `default-features = false`, and `default_fonts` is one of those defaults, so egui had no fonts at all. Every shape drew and there were no glyphs to paint. | "text is STILL black", three times, and it was never black and was never text | `there_are_fonts_and_they_produce_actual_glyphs`, and `both_font_families_are_present` |

## bug 10, in full

The Command Panel came up with the right background, the right accent, borders, status pips and
hairlines all correct, and not one letter anywhere on it. JJ reported it as black text on a
black background, which is exactly what it looks like, and it was neither black nor text.

Three wrong diagnoses before anybody read the manifest, and the wrong turns are the useful part.

**The desktop theme.** The machine is in light mode, egui follows the desktop and reapplies
that every frame, so setting visuals once loses a fraction of a second later. That is a real
bug and it was fixed and it was not this one. `theme::install` now sets `ThemePreference::Dark`
and runs every frame.

**The renderer.** Shapes drawing and glyphs not drawing is the signature of a font atlas that
never reached the GPU, so the OpenGL backend was blamed and the whole thing was rebuilt on
wgpu. It changed nothing, because the renderer was never involved.

**The colours.** A headless check walked the render output and confirmed the panel painted text
at `#D6DEE9` on `#06080B`. That check was correct and useless: it inspected the colour each
text shape asked for, and by construction it cannot see that there were no glyphs to paint in
it. A test that cannot fail for the real reason is worse than no test, because it is evidence
pointing the wrong way.

What solved it was a screenshot. Shapes present, glyphs absent, identical under two different
renderers, which is a font question and not a colour one and never was.

The guard measures glyphs rather than intent. It lays out four letters in every text role and
in both families, and asserts the laid out width, the height and the glyph count. Removing
`default_fonts` was tried again afterwards to be sure it bites: it fails with "laid out no
width, which means no font is loaded", and passes with the feature restored.

The general lesson is about `default-features = false`. It is reached for to keep a dependency
small, and it silently drops things that were never thought of as optional. Fonts are not a
feature of a graphical toolkit in any sense a person would recognise.

## bug 9, in full

The worst kind, because the fix made the thing worse than before and was reported as working.

Carl was giving vanilla advice to somebody whose mods directory held Sea Block, Angel's,
Bob's and Space Exploration. Reading that directory looked like the obvious fix, and the new
answer talked about crushing, floating and leaching, which is Angel's ore processing. It read
as a clear improvement and was shown as proof.

None of those mods were switched on. Factorio keeps what is downloaded and what is enabled in
different places, and `mods/mod-list.json` is the one the game reads. Eighty eight downloaded,
four enabled, and three of those are the official Space Age components.

So the advice went from being right about the wrong game to being wrong about a game nobody
was playing. Vanilla advice on a vanilla save was closer to correct than the confident,
detailed, entirely inapplicable answer that replaced it.

JJ caught it, not a test, and not the person who wrote it. The lesson is not about Factorio.
A file being on disk is not the same as it being in use, and the difference is exactly the
kind of thing that produces a confident answer about something that is not there.

| 10 | Any sentence containing "that", "this" or "here" was treated as pointing at the screen, so an ordinary conjunction took a screenshot. | "Remember that my mentor is called Hunter Zhang" flashed the screen, captured a black image, and spent vision tokens describing it | `a_conjunction_is_not_somebody_pointing_at_the_screen` |

| 11 | An interrupted append to the milestone file left it without a trailing newline, so the next append was glued onto the broken line and both were lost. | One crash cost two milestones, and the second was the one somebody had just recorded and believed was safe | `a_truncated_line_costs_only_itself_and_the_next_appends_survive_a_reopen` |

## bug 11, in full

Found by Process 3 while writing durability tests for the project store, not by anything failing.

Milestones are one JSON object per line. A write cut off part way through, by a crash or a full
disk, leaves a file that does not end in a newline. The next append then writes straight onto the
end of that broken line, and the reader sees one unparseable line where there were two records.

So an interruption cost **two** milestones rather than one, and the second was the worse loss: it
was written after the machine came back, by somebody who had every reason to believe it was
safely on disk.

The fix is to close the boundary before writing. If the file does not end cleanly, a newline is
appended first, then the new record. The damaged line stays damaged and stays counted by
`milestone_gaps`, which is honest and visible, but it no longer takes its successor with it.

The general shape is worth remembering: in an append only text format, a torn write is not a
local problem. It corrupts the *boundary*, and a boundary is shared with the record that comes
next. Anything that appends lines and does not check how the file ends has this bug.

The army journal appends the same way and does not have it, which is luck rather than design:
`Journal::append` writes through `writeln!` on a freshly opened handle and has never been
interrupted mid line in practice. Worth revisiting if it ever grows a buffered writer.

## bug 10, in full

Found by reading the conversation record rather than by anything failing.

`needs_screen` treated every pointer word as deictic. That is wrong about English by a wide
margin: "that" is a conjunction far more often than it is a pointer. "Remember that", "make
sure that", "I think that", "it turns out that".

So an ordinary sentence took a screenshot. It flashed the display, which GNOME gives no way to
suppress, captured a black image because the screen happened to be off, and then spent a few
thousand vision tokens having Carl describe the black image back.

The comment above the function already said it errs toward not looking, and the implementation
did the opposite. A comment describing behaviour the code does not have is worse than none,
because it stops anybody checking.

A pointer now counts only when it genuinely points: at the end of the sentence, after a
preposition aiming at it, or in a question short enough that there is nothing else it could
mean. "is this right" looks. "remember that my mentor is called Hunter Zhang" does not.

## bug 7, in full

The one worth reading, because every check said the audio was fine.

A canceller was started by hand early on, and later the same canceller was started again as a
systemd service. Both create a sink called `carl-speaker` and a source called `carl-mic`.

Naming a device by name then picks whichever the audio server feels like, and there is no
reason for the two choices to belong to the same canceller. Carl played into one and recorded
from the other. The one listening had no idea what the one playing was doing, so it
subtracted nothing, and Carl heard himself at full volume.

What made it hard to see is that every diagnostic said the right thing. `Devices::detect`
found a sink called `carl-speaker` and a source called `carl-mic`, exactly as it was written
to, and reported echo cancelled audio. The service was active. The nodes existed. The audio
was not cancelled.

It looked like this in the journal, each reply becoming the next question:

```
08:14:52  Carl says:  Good, thanks.
08:14:54  Carl hears: "Good, thanks."
08:14:59  Carl says:  Glad to hear it.
08:15:01  Carl hears: "Glad to hear it."
08:15:08  Carl says:  Are we doing echoes now?
```

The fix is to count rather than to look. Exactly one of each is the cancelled case. Zero is
refused, which it already was, and now two or more is refused as well, with a message naming
the spare process. Falling back to muting the microphone is worse audio and obviously correct
behaviour, which beats better audio that silently is not.

This is the third time the same shape has appeared in this project, after the microphone
hearing the speakers and Carl reading his own Slack messages. It is the first time it got
through a guard that was written specifically for it.

## The overlap detector was reporting overlaps nobody could see

The layout tests rest on `probe::collisions`, which finds text painted on top of other text.
It compared the rectangles egui laid rows out into. That is not what reaches the screen. A
scroll area lays out every row it holds and shows the few that fit, and egui records a paint
command for the rest with a clip rectangle that throws them away. So the rows below the fold
were compared against whatever was drawn down there, and reported as landing on it.

The visible cost was a phantom defect. The rail's summary was said to run into its own footer
at 1280x800. It never did. Four attempts to fix the rail failed, one after another, because
there was nothing wrong with the rail, and the size was eventually excluded from the overlap
check with a note in the report calling it a real defect that needed the rail rethinking.

The hidden cost was worse. Excluding a size to get past a false positive is exactly how a real
one stops being caught, and there was a real one behind it: `card::sized_card` called
`ui.set_clip_rect(inner)`. That replaces the clip rather than narrowing it, so a card sitting
half below the fold of a scroll area handed its contents a clip reaching past that area, and
they painted over whatever was underneath. With the workspace open at 1280x800 the overview
columns drew on top of the workspace header, and CLOSE landed on an agent's status chip.

Two fixes, and the order matters. `collisions` now intersects each row with its own clip and
skips what is left with nothing, so it reports what a person can see. Both clip overrides now
intersect with the parent clip instead of replacing it. Then the excluded size went back into
the overlap check, the empty state check and the workspace check, and they pass.

`text_the_clip_threw_away_is_not_reported_as_an_overlap` asserts that both rows were recorded
and stacked before asserting that no collision is reported, because a test that could pass by
never painting the second row would prove nothing.

## Finished reasoning still said thinking

The panel ignored the finished state when reasoning arrived as a token count
with no text. It now draws a finished reasoning label.
`finished_redacted_reasoning_is_drawn_as_finished` inspects the rendered text
for both states. It failed on the old label before the fix.

## Carl was not told about his organisation

The identity now names the leads, separates questions from work and routes
Miles through Olivia.
`the_chief_is_told_to_delegate_work_through_his_leads` guards that instruction.
It fails when the organisation section is removed. This checks the brief,
not a live model decision. The local delegation permission still needs approval.

## Approval answers waited behind the turn they needed to release

The panel ran every command on one blocking queue. A tool waiting for approval kept the
current command open, so Allow could not reach the backend until that command ended.
Backend logs showed answers arriving after their ten minute expiry. Permission answers now
have a separate worker and cannot close Carl's streaming turn. The panel waits for the
backend's confirmation before claiming that a tool was allowed.

`an_allow_reaches_the_backend_before_the_waiting_turn_finishes` failed against the old queue
with Allow stuck behind the waiting turn. `answering_permission_does_not_finish_carls_turn`
and `an_allow_click_waits_for_confirmation_before_claiming_success` cover the display state.

## Rank filtering removed the delegation route but left implementation tools available

The filter compared the bare name Bash with the full scoped handoff rule, so Carl lost a
handoff permission JJ had explicitly granted. The chief now keeps exact scoped permits.
`the_chief_keeps_the_exact_handoff_permit_without_getting_a_shell` failed against the old
filter, which returned an empty list.

Allowed tools only control automatic permission. They do not remove tools from Claude's
context. Carl now receives an explicit built in tool ceiling and a role hook that rejects
implementation commands even in permissive modes. Existing scoped approvals are honored.
Held sessions now carry that hook too. Tests cover the command ceiling, safe delegation,
rejection of shell composition and same surface approval reuse. The worker Write approval
transport test still exercises the full socket round trip under the worker's own identity.

## Resumed Carl conversations mistook disabled agent tools for a missing AOS route

Each request now includes the current Bash handoff route, independently of session history.
Olivia's brief explicitly names her handoff to Miles. The regression
`resumed_turns_carry_the_current_handoff_route_without_rewriting_history` failed against the
previous preparation code and now covers fresh and resumed requests and unchanged human logs.

## Terminal provider errors left held sessions waiting forever

The stream parser discarded error results. A held process could wait for another request
while its caller waited for a result that had already arrived. Errors now terminate the turn
through held sessions, one shot streams and direct panel agent requests. Failed chain turns
also close their activity record. `miles_failure_reaches_carl_as_failure` reproduced the
lost failure with real nested handoff processes and a local deterministic provider fixture.
`carl_olivia_miles_handoffs_return_the_workers_result` covers the successful return path.

## Worker hooks asked again for tools already permitted by rank

Miles's live send required repeated approvals for memory reads, tool discovery and Gmail.
Named leads and workers now return no overriding hook decision for tools already in their
rank's allowed list. Claude still applies its permissions and other hooks. Exact ToolSearch
selections containing only permitted Gmail tools can load without a second panel question.
Unlisted tools and unknown surfaces still reach the approval path.

`a_preapproved_worker_read_does_not_wait_for_the_panel` failed against the old hook with a
Read denial. The nested process fixture now invokes the real hook for its read too.
`existing_rank_permissions_defer_to_the_cli_without_overriding_denies` and
`gmail_discovery_is_limited_to_exact_permitted_names` cover the boundaries. The full socket
approval test retains its Write request and assertions under Olivia, who has no preapproved
Write, so it still verifies a tool that actually needs approval.

## Successful reads containing permission rules appeared as refusals

The parser searched successful tool output for the word permission. Reading the memory index
therefore produced a false refusal and copied the whole index into activity. Refusal parsing
now requires is_error to be true. `reading_permission_rules_is_not_a_tool_refusal` failed
against the old parser for successful output with both absent and false error flags.

## An established panel stream missed journal replacement

Gap detection ran only on subscription. Replacing the journal after connection left the old
snapshot visible forever. The stream now checks its position on every journal poll. The
existing `a_resynced_snapshot_replaces_provider_state_rather_than_merging_it` test now observes
a barrier event before replacing the journal and has a deadline. All original snapshot
assertions remain. It failed deterministically against the old stream after twelve seconds.


## Harmless command queries asked for permission or were refused

The panel hook now automatically permits literal pwd, whoami, hostname, uname,
date and uptime queries with explicitly checked options. Shell composition,
redirection, expansion, wrappers and mutating options do not match. Other calls
retain the existing role and permission checks, and the separate guard still runs.
The regression test harmless_shell_queries_do_not_need_a_panel_click failed on
the old hook before the fix. composition_and_mutating_options_never_auto_approve
covers attempts to hide another action inside a query.


## Directory inspection still stopped for permission

The inspection policy now accepts ls, stat, df, du, free and id with explicit
options. Literal quoted file paths are supported. Unknown options, shell
expressions, redirects and executable wrappers retain existing checks.
directory_and_metadata_queries_do_not_need_a_panel_click failed on the previous
policy before the fix. file_inspection_refuses_execution_tricks_and_unlisted_options
checks the boundary, including repeated free output and alternate input sources.

## Deleted executable broke every Claude hook

The dynamic hook used `current_exe()` verbatim and interpolated unquoted arguments. Replacing a running Carl binary added ` (deleted)` to its process path and made the shell reject every tool call. Resolve the configured filesystem path first, remove the deleted marker, quote every argument, and emit an explicit deny decision if the executable is missing or fails. The permission policy and static guard are unchanged.

Regression tests: `hook_paths_and_all_arguments_survive_shell_metacharacters`, `deleted_marker_uses_the_replacement_binary_on_disk`, `missing_binary_returns_explicit_deny_json_with_successful_hook_exit`, and `an_executable_that_fails_cannot_make_the_guard_disappear` all failed against the old implementation. `a_running_unlinked_process_uses_the_installed_replacement` also reproduces a real Linux executable replacement while its process remains alive.

## Arch Python sandbox startup

The sandbox required the Debian directory `/etc/alternatives`, which is absent on Arch. Its optional read only mount now permits absence while retaining every isolation flag. `arch-migration/test-migration.py::MigrationTests::test_python_sandbox_handles_missing_debian_directory` failed against the old script and passes with the fix.

## Status notice overlapping the title

The header let status text expand backwards over CARL when clocks consumed its space. Status notices now occupy their own wrapping band below the title. `refusal_notice_has_its_own_row_below_the_carl_title` failed against the old drawing code and passes at three window widths.

## Common grep searches required approval

`common_grep_searches_are_approved_without_shell_execution` failed before adding the literal grep parser. Regular expression arguments and ordinary read options now pass automatically, while command substitution, redirection, composition and unknown options are refused by the automatic policy. The existing metadata policy also enables ls alongside pwd. `directory_and_metadata_queries_do_not_need_a_panel_click` exercises the complete hook.

## Iris issue workflow

Iris had instructions but no controlled issue investigation path. `iris_brief_routes_investigation_through_controlled_workflow_only` failed against the previous brief. `iris_cli_preserves_request_and_workflow_flags` checks the actual CLI interface. Publication is serialized and reconciled after an ambiguous response. `test_closed_marker_prevents_duplicate_creation` fails when duplicate protection is removed. `test_draft_then_publish_same_head_reuses_reviewed_plan` caught publication being skipped after a draft run. `test_inline_credentials_never_enter_model_batches` fails without credential screening. GitHub pagination tests reproduce the installed CLI rejecting `--slurp` and validate concatenated JSON pages instead.

`test_focused_request_does_not_suppress_general_scan` failed when request scope was omitted from completion records. A focused inspection can no longer suppress the next general scan. `test_poll_checks_new_commit_without_waiting_an_hour` checks immediate eligibility for commits without a feature prefix.

The first live investigator returned a mismatched excerpt and publication failed closed. Investigator inputs now include explicit line numbers instead of asking the model to count an entire file. `test_no_findings_creates_no_issues` checks the numbered evidence prompt, and `test_invalid_excerpt_never_publishes` retains the rejection boundary.

## Iris process failure diagnostic

A live investigator exited without its reason reaching the report. The CLI JSON failure subtype or sanitized stderr now accompanies the exit code. `test_failed_process_reports_budget_reason_without_exposing_credentials` fails against the old generic message and verifies that credential text is excluded.

## Iris investigation exhausted its call budget

A live 29 file investigation returned `error_max_budget_usd`. Default batches now contain at most eight files and 64 KiB of source, and reasoning effort is explicitly medium. `test_default_batches_fit_the_live_model_budget` and `test_tool_free_argv_and_reservation_before_process` fail against the old settings. `test_timeout_reports_duration_without_dumping_prompt_or_command` also fails against the old timeout diagnostic. Source validation and independent review remain required.

## Room renders duplicate messages when polls overlap

Iris reproduced a browser defect against the committed portal source. `integrations/iris/browser-tests/tests/portal.spec.js` test `overlapping polls display each message once` expects one DOM message and receives two after a delayed response. The regression remains failing until the application is fixed. The other five browser cases pass. Independent review accepted the local issue draft in `integrations/iris/findings/overlapping-polls.md`.

## Notification acceptance was mistaken for visibility

The desktop service returned an ID on Ubuntu, but JJ was using Arch and did not see the notice. `test_acceptance_does_not_claim_visibility_and_notice_is_persistent` fails against the old implementation. Reports now distinguish service acceptance from unknown user visibility, and notices request persistence. `test_idle_poll_does_not_hide_published_issues_or_browser_failure` guards a durable overview that keeps issue links and test failures visible after routine polls.

## Iris findings buried the visible problem

Issues now lead with what the user sees and retain an explicit statement when runtime testing has not occurred. `test_issue_leads_with_user_visible_problem_and_labels_unexecuted_test` fails against the old template. The overlapping poll test now checks the stored count separately from the displayed count. Three original runs showed one stored message and two displayed messages. Three isolated comparison runs showed one of each.

## Iris status required write access

Status now renders the journal without writing to the state directory. `test_status_does_not_write_to_the_state_directory` fails against the old CLI. Both new Python regressions pass with the fixes, along with all 47 workflow tests.

## Iris manual requests and review reports

`test_specific_issue_checks_only_selected_committed_source_and_reuses_draft` verifies the new
specific issue route and exact committed file selection. `test_existing_closed_finding_reports_link_without_new_issue`
keeps prior issue links visible. `test_new_closed_issue_blocks_stale_reviewed_plan` and three other
publication regressions fail against the old publication function. Fresh history is checked before
resuming publication. Models and GitHub writes in these tests are mocked.

`test_daily_budget_reports_actual_reason_without_calling_model`,
`test_discovery_failure_finishes_status_and_preserves_failure`,
`test_no_findings_is_explicit_and_manual_report_survives_poll`,
`test_minor_findings_group_by_file_without_burying_major_findings`, and
`test_suggested_direction_is_distinct_from_acceptance_criteria` all fail against the prior code.
All 66 workflow tests pass with these changes. One real committed file review produced three
local drafts. Operator reproduction corrected one weak proposed example without publishing it.
See `integrations/iris/manual-verification.md` for the exact boundary of that evidence.

## Iris requests stay in Carl chat

`iris_work_stays_in_carl_chat_and_goes_through_adrian` fails with the previous capability brief.
Carl now routes Iris work to Adrian, and Adrian routes it to Iris. The worker preserves scope
and draft intent and uses the status command for status questions. The existing resumed-turn
regression also checks that the Iris route reaches subsequent panel conversation turns without
changing the recorded user message. Verification uses the established panel socket and delegation
chain, with no separate Iris interface or expanded tool permissions.

## Completed Iris answers were trapped during cleanup

The real panel request reached Iris, who recorded her answer, but the handoff waited for process
and stdout cleanup instead of returning it. `completed_handoff_does_not_wait_for_inherited_stdout`
fails when reader cancellation is removed. `panel_turn_returns_final_answer_without_waiting_for_stdout_eof`
fails when final answer handling resumes waiting for EOF. Both regressions take about four seconds
without the fix and finish promptly with it. `completed_session_has_a_deadline_when_child_ignores_eof`
checks the bounded child exit grace. The real retry returned through Iris, Adrian and Carl.

`test_published_draft_is_not_reported_as_unpublished_with_legacy_marker` fails against the prior
status renderer. Publication now removes a matching draft from the unpublished list. Carl's
instructions explicitly forbid diagnosing killed handoffs from the review allowance.

## Omarchy audio routing and recorder failure

The cancelled microphone and speaker requested ALSA's Pulse plugin, which is not
installed on Nexus. The recorder exited and calibration waited forever while
systemd reported the listener as running. Named devices now use PipeWire's ALSA
node directly. Both Ubuntu and Omarchy expose that backend.

`named_microphone_uses_pipewire_without_the_pulse_plugin` and
`the_named_sink_uses_pipewire_without_requiring_the_pulse_plugin` fail with the
original Pulse route restored. `exited_recorder_is_reported_instead_of_hanging_calibration`
and `stalled_recorder_is_reported_before_the_service_hangs` fail when the exit and
deadline checks are removed. Both reversions were exercised before restoring the
fix. Recording errors now reach the service log and stop the listener.

The room service units also pointed at the historical AOS/carl checkout and allowed
only 600 seconds for a permission hook that can wait 660 seconds. The executable
path checks and existing timeout checks in `portal/check-services.sh` failed against
those units. Both units now use the active checkout and allow 960 seconds.

## Unmeasured diagnostics looked like work needing attention

The default diagnostic cards treated unavailable readings as urgent investigation work.
The live system has no completed handover latency samples and no readable thermal sensor,
which are measurement gaps rather than reported faults. Missing readings remain visible
in an expandable group, while the attention count covers faults and stale healthy samples.
`unmeasured_components_do_not_pose_as_faults_requiring_action` failed before the change.
The existing missing reading visibility test still passes without removing its assertion.

## Completed assignments obscured current work and Evan reviews

The agent inspector fell back to the first owned task even when it was accepted or abandoned.
That made an idle agent display finished work, hid newer assignments, and prevented Evan's
prepared repair card from appearing after any prior task. The selection now requires an
unfinished task owned by the selected agent, including when an old task ID is still present.
`finished_assignments_do_not_show_as_an_agents_current_task` and
`an_agents_current_task_skips_finished_history` failed before the selection was corrected.
The existing prepared repair action test also includes a completed assignment now.

## A delegated assignment hid Evan's separate repair workflow

The task list deduplicated everything by agent, and the inspector displayed a repair only
when no delegated task existed. A current assignment could therefore hide a ready repair.
`an_evans_assignment_does_not_hide_his_separate_repair_review` failed before the change.
The same compact workflow card now appears in To-do and the inspector alongside real tasks.
A workflow is counted once and never inserted into the conversational task journal.

## Evan reports hid ready repairs and treated unknown states as idle

A report containing both blocked and prepared issues displayed only the blocker.
`a_blocked_issue_does_not_hide_another_prepared_repairs_review_action` failed before
the report retained the prepared repair summary and its review command. Blocked work
still determines the overall health, so a ready repair cannot clear an actual blocker.

An unrecognized status also fell through to healthy idle.
`unknown_evan_report_status_does_not_claim_an_idle_healthy_workflow` reproduced that
false claim. Unknown report vocabulary now produces an unavailable status.

## Carried over Carl issues in the current AOS implementation

Archived Carl issue 42. A fallback tool payload longer than 200 bytes was sliced inside
a Unicode character. `a_long_unicode_tool_payload_is_denied_without_panicking` reproduced
the panic before truncation was moved to a character boundary. The hook now returns its
ordinary deny response when no backend can answer.

Archived Carl issue 45. Selecting JJ returned an empty inspector because the backend
correctly excludes humans from the agent snapshot.
`selecting_jj_shows_human_authority_instead_of_an_empty_inspector` failed before the fix.
JJ now has a human authority card, and a missing agent gets an explicit unavailable reading.

Archived Carl issue 41. Handover latency included review and later submissions.
`handover_latency_ends_at_first_submission_not_review_or_resubmission` reproduced 7000
seconds where the first handback took 100. The fold now retains delegation and first
submission times separately from total lifetime.
`a_submission_without_a_recorded_delegation_has_no_measured_latency` prevents a missing
delegation from becoming a fabricated zero second handover. Both failed before the fix.

Archived Carl issue 47. CPU totals counted guest time twice.
`guest_cpu_time_is_counted_once_in_total_and_busy_fraction` failed before summing only the
first eight fields. Linux accounts guest time in user and nice already, as shown in
[account_guest_time](https://github.com/torvalds/linux/blob/master/kernel/sched/cputime.c).
The fixture now reports 60 percent busy instead of the inflated fraction.

The same issue identified a task capacity check that only counted tasks in hand.
`unfinished_started_tasks_keep_the_workers_capacity` failed for submitted work before the
check was corrected. Submitted, changes requested and blocked work now reserve the worker
until acceptance or abandonment. Assigned tasks remain queued, preserving the public API's
existing queue tests and its distinction between lining up work and starting it.

Archived Carl issue 43. Each conversation registry rewrote its original snapshot after
answering, losing threads or turns saved by another process in the meantime. Mutators now
lock the containing directory, reload the current registry, and save while still locked.
The lock is held only for the file operation and survives the atomic registry rename.
`finishing_an_answer_preserves_a_thread_created_by_another_registry`,
`stale_registries_reuse_the_same_new_session_and_preserve_turn_counts`, and
`concurrent_registry_writers_preserve_every_session` all failed against the previous code.
The recency fixture now saves its manually assigned times before a mutation reloads them.

## Voice bugs carried over from the archived Carl backlog

Issue 37. Completed playback skipped cleanup of the Piper child.
`dropping_finished_playback_reaps_piper_even_when_it_is_still_running` failed before
Drop always cleaned up both owned children. The test uses silent disposable processes
and covers a synthesizer that has exited and one still running after playback ends.

Issue 38. The final fragment of a streamed code block lost its fence state.
`a_stream_ending_at_or_inside_a_code_fence_does_not_repeat_or_read_code` failed before
the trailing speech conversion inherited that state. An ending fence is no longer
announced again, and interrupted code is not read aloud.

Issue 39. The shorter Carl spelling matched inside Carle and left a stray letter.
`the_longer_wake_name_leaves_no_letter_in_the_question` failed before matching complete
wake words. `a_wake_name_inside_an_unrelated_name_does_not_start_a_conversation` also
failed for Carlton. Both now pass without changing the supported wake spellings.

## Invisible line ending changes in the editor

Archived Carl issue 44. Line splitting discarded both a final newline and CRLF, leaving
an empty diff header even though the editor buffer differed from the saved file.
`adding_or_removing_the_final_newline_shows_the_changed_line` and
`changing_crlf_to_lf_visibly_identifies_the_line_ending` both failed before preserving
line terminators for comparison and rendering readable labels for their differences.

The full suite also exposed a test fixture lock outliving its descriptor because parallel
PTY tests can inherit it between fork and exec. The workflow lock test now explicitly
unlocks its owned descriptor before asserting the same interrupted workflow behavior.
No assertion was removed or relaxed.
