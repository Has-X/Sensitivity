# Localization source and translator guide

This document is the authoritative English context guide for every
user-facing Sensitivity surface. Translate the meaning and safety intent, not
the individual English words. Never translate protocol literals, command names,
file extensions, option names, JSON fields, product names, device codenames, or
the recovery-mode label `Connect with Mi Assistant`.

For the live coverage matrix and release checks, see
[LOCALIZATION_STATUS.md](LOCALIZATION_STATUS.md).

## Canonical sources

| Surface | English source | Runtime files | Notes |
| --- | --- | --- | --- |
| Windows app | `locales/_keys/windows.json` | `locales/<language>/windows.json` | The semantic key is stable. The English value is the translation source. |
| Portable GUI | `locales/_keys/gui.json` | `locales/<language>/gui.json` | Uses the same semantic-key model. |
| Windows installer | `installer/Sensitivity.iss` | Inno Setup language files and `[CustomMessages]` | Preserve silent-install compatibility and accelerator syntax. |
| CLI | `locales/en/cli.json` | `locales/<language>/cli.json` | `SENSITIVITY_LANG`, `LC_ALL`, or `LANG` selects the human-language catalog. Keep machine JSON unchanged. |

All supported languages live under `locales/<language>/`. Each language keeps
the same three surface files: `cli.json`, `gui.json`, and `windows.json`.
Platform code reads its surface file directly, so translations stay together
without mixing unrelated keys. Shared semantic alias maps live in
`locales/_keys/`. The currently supported language directories are `en`, `hu`,
`es`, `de`, `fr`, `it`, `pl`, `pt-BR`, `pt-PT`, `tr`, `id`, `ro`, `cs`, `sk`,
`ru`, `uk`, `zh-CN`, `zh-TW`, `ar`, `vi`, `th`, `hi`, `ja`, `ko`, `nl`, `el`,
`bg`, `hr`, `sr`, `sl`, `sv`, `da`, `fi`, and `nb`.

English is the source language. Do not use Hungarian or Spanish as a fallback
source. Add a new semantic key before adding a new user-visible sentence.

## Key and message context

| Prefix | Meaning | Translation rule |
| --- | --- | --- |
| `app.*` | Product identity and short tagline | Keep `Sensitivity` unchanged. The tagline is friendly, short, and not a safety instruction. |
| `nav.*`, `page.*`, `section.*` | Navigation, page titles, section headings | Use concise noun phrases. Avoid sentence punctuation unless the source has it. |
| `label.*` | Field labels | Keep technical identifiers such as USB, ROM, ADB, MD5, ZIP, and WinUSB recognizable. |
| `action.*` | Buttons and menu actions | Use an imperative verb. Never soften destructive actions such as erase or flash. |
| `status.*` | Transient progress or result text | Use a short present-progress phrase for work in progress and a past-result phrase for completion. Keep the ellipsis character if present. |
| `connection.*` | USB and recovery discovery | A recovery interface is a USB interface, not necessarily a fully connected phone. |
| `rom.*`, `profile.*` | ROM selection and regional identity | Do not translate file extensions, codenames, region codes, or Xiaomi validation terms. |
| `dialog.*` | Confirmation dialog title and detail | Preserve the distinction between a review step and the final destructive approval. |
| `error.*` | Recoverable problem text | State the user action first. Do not expose raw serials, validation tokens, or private URLs. |
| `setting.*` | Persistent preference | Describe the effect, not implementation detail. |
| `guide.*`, `flow.*` | Guided recovery sequence | Maintain the stated order. Do not remove safety warnings. |
| `about.*` | Product and publisher information | Keep `Chromatic`, `chromatic.hu`, and `feedback@chromatic.hu` unchanged. |

## Placeholders and formatting

The following placeholders are substituted at runtime and must remain exactly
unchanged: `{count}`, `{device}`, `{version}`, `{region}`, `{romzone}`, and
`{error}`. Preserve the surrounding whitespace needed by the target language.

`{count}` is a decimal interface count. `MD5` is an uppercase checksum name.
`ROM` means a Xiaomi Recovery ROM package, never a generic read-only memory
term. `flash` means transfer and install an approved recovery package. It must
not be translated as a visual camera flash.

## Safety-critical wording

- **Flash** transfers an approved Recovery ROM to stock recovery.
- **Erase all data** permanently formats user data. It is irreversible.
- **Cancel safely** requests a graceful stop after the current USB operation.
  It does not promise that the phone has already stopped.
- **Validate** means Xiaomi service approval and checksum verification before
  transfer. It is not a claim that every device or package is safe.
- **WinUSB** and **ADB** are technology names and remain untranslated.

## Adding or changing a string

1. Choose a semantic key in the appropriate prefix family.
2. Add the canonical English text to the English source and every currently
   supported locale. Do not ship a generic machine-translation artifact. If a
   safety string cannot be reviewed, retain the clear English source text and
   list it for native-language review rather than inventing a misleading term.
3. Add or update the context above if a translator could misread the action,
   placeholder, protocol, or safety implication.
4. Run the platform build and check that the text fits at narrow window sizes.
5. Keep CLI `--json` field names and machine event identifiers stable. They are
   automation contracts, not display text.

## Validation

Run `pwsh -NoProfile -File tools/check-locales.ps1` before committing. It
checks every locale for identical key sets, non-empty values, placeholder
integrity, and known machine-translation artifacts. Run
`pwsh -NoProfile -File tools/preflight.ps1` for the full Windows release
preflight, including a self-contained publish and installer compile when ISCC
is installed.

Low-level USB protocol diagnostics intentionally retain stable English terms
such as ADB, WinUSB, CNXN, and sideload-host. These are technical identifiers,
not translatable UI copy.
