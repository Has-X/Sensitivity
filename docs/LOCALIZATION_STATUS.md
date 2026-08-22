# Localization status

Status date: 2026-08-22

## Runtime coverage

Sensitivity now ships catalogs for every approved rollout language across the
CLI, portable egui GUI, and native Windows UI:

`en`, `hu`, `es`, `de`, `fr`, `it`, `pl`, `pt-BR`, `pt-PT`, `tr`, `id`, `ro`,
`cs`, `sk`, `ru`, `uk`, `zh-CN`, `zh-TW`, `ar`, `vi`, `th`, `hi`, `ja`, `ko`,
`nl`, `el`, `bg`, `hr`, `sr`, `sl`, `sv`, `da`, `fi`, `nb`.

The Windows picker accepts the same set. The CLI detects `SENSITIVITY_LANG`,
`LC_ALL`, or `LANG`; Portuguese and Chinese region tags are resolved to the
matching Brazilian/European or simplified/traditional catalog. Existing ROM
region profiles remain independent from UI language selection.

## Validation

`tools/check-locales.ps1` compares every locale with the English source for
missing keys, extra keys, empty values, and placeholder mismatches. It validates
all three catalogs for every locale. Machine-translated strings are a complete
runtime baseline, but native review is still recommended before publishing
language-specific screenshots or documentation, especially for recovery, erase,
flash, and validation warnings.

Arabic is included as a catalog and selector option. Before release, the Windows
layout must be checked in RTL environments at 100%, 125%, 150%, and 200% DPI.
The same visual pass is needed for Chinese, Japanese, Korean, Thai, and Hindi.

## Installer and documentation

The application catalogs support all languages above. Inno Setup message packs
are enabled only where the installed Inno compiler provides a matching `.isl`
resource; unavailable installer message packs fall back to English while the
installed application still uses its selected catalog. Documentation remains
English-first until native contributors review each translation.

## Adding another language

1. Add `cli.json`, `gui.json`, and `windows.json` under `locales/<code>/`.
2. Keep keys and `{placeholders}` identical to `locales/en/`.
3. Run `pwsh -NoProfile -File tools/check-locales.ps1`.
4. Register the code in the Windows resolver and portable GUI list.
5. Run formatting, tests, and both desktop builds before release.
