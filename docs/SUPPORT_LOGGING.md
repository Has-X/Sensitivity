# Support logging

Windows release builds log sessions automatically under
`%LOCALAPPDATA%\Sensitivity\Logs`. In **Diagnostics**, **View session log** shows
the current timeline. **Export bug report** saves a ZIP of the current session
and up to two recent sessions, so a restart after an error does not erase the
evidence. Include what you clicked, what you expected, and what happened.
Export is local; Sensitivity does not upload reports automatically.

Each report includes application/backend versions, OS and architecture,
operation names, start/end timestamps, durations, exit codes, cancellation,
HTTP status codes, download mirror attempt numbers, checksum outcomes,
recovery query names, validation response structure with scalar values
redacted, transfer milestones, and application error categories and stack
method names. The logger never records raw stdout/stderr, command arguments,
device serials, tokens, ROM paths, private URLs, environment-variable dumps,
or raw USB/HTTP payloads. Error categories preserve facts such as HTTP 403
without copying a signed download link into a report.

The Windows logger retains the ten most recent session directories. The UI
timeline is capped at 2 MiB and each backend log at 5 MiB; an export is capped
at 25 MiB and the in-app preview at 150,000 characters. Limits can omit older
events. Files use the current user's local application-data access controls.
If logging is unavailable, Diagnostics says so; recovery work is still usable.
Review reports before sharing them. Close the app before removing its logs.

For the CLI or portable GUI, set `SENSITIVITY_LOG_FILE` to a file in an existing,
writable folder before starting the program. For example, in PowerShell:

```powershell
$env:SENSITIVITY_LOG_FILE = Join-Path $PWD 'sensitivity-session.jsonl'
.\sensitivity-cli.exe doctor
Remove-Item Env:SENSITIVITY_LOG_FILE
```

CLI log files are opt-in and are not covered by Windows session retention.
`SENSITIVITY_LOG_ROOT` optionally overrides the Windows app's log root for
isolated development/testing. Do not point it at another application's folder.

These reports describe software stages and outcomes. They do not prove a real
flash or establish why a particular Xiaomi server rejected a signed URL. A 403
tries remaining server-advertised URLs with their signing parameters intact.
It also searches public Xiaomi CDNs (`bigota`, `hugeota`, `cdnorg`, `bn`, and
`ultimateota` under `d.miui.com`) for the same artifact path and versioned
filename. Public candidates get a bounded four-byte range probe and must return
a ZIP signature, rather than an HTTP error page. Private signing parameters are
never copied onto these public URLs. A successful probe is only availability
evidence: the full download must still match Xiaomi's expected MD5, and flashing
still needs Xiaomi validation. If all candidates fail, retry later or manually
obtain the official Recovery ROM. An available mirror can become unavailable;
the application checks it at the time of use rather than promising a permanent
working host list.

Maintainers can repeat the same network probe without downloading a full ROM:
set `SENSITIVITY_MIRROR_TEST_PATH` to a public versioned path without query or
fragment, then run `cargo test -p sensitivity download::tests::live_public_cdn_health --locked -- --ignored --nocapture`.
This opt-in test uses the application's actual range-probe implementation and
requires at least two mirrors to return ZIP bytes. It is excluded from normal
offline tests because availability changes independently of the source.
