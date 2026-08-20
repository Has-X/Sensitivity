# Security policy

## Supported versions

Security fixes are applied to the latest released version and the active development branch.

| Version | Supported |
| --- | --- |
| Latest release | Yes |
| `dev` | Yes |
| Older releases | Best effort only |

## Reporting a vulnerability

Please do not report security vulnerabilities in a public issue, pull request, discussion, or chat message.

Use GitHub's [private security advisory form](https://github.com/Has-X/Sensitivity/security/advisories/new). If that form is unavailable, contact the maintainers through the private contact method listed in the repository profile and include the word `Sensitivity security` in the subject.

Please include:

- the affected version or commit;
- the operating system and interface involved;
- a short description of the impact;
- reproducible steps or a minimal proof of concept;
- any suggested mitigation.

Do not include real device serial numbers, validation tokens, private ROM URLs, complete raw recovery responses, or unredacted USB captures. Replace those values with placeholders before submitting evidence.

## What is in scope

Reports are welcome for vulnerabilities in the Rust core, CLI, portable GUI, Windows application, release packaging, GitHub Actions, or documentation that could cause unauthorized device access, data loss, token disclosure, unsafe recovery actions, or compromise of the build and release process.

## Response process

We will acknowledge a report within seven days, investigate it privately, and coordinate a fix and disclosure timeline with the reporter. A report may receive a security advisory and a credit unless the reporter asks not to be named.

The recovery tool is intentionally conservative: it does not unlock bootloaders, bypass account or FRP protections, or expose arbitrary raw ADB commands. Reports that demonstrate a way around those boundaries are especially important.
