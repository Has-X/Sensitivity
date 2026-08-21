# Localization status

Status date: 2026-08-22

## Current support

| Surface | English | Hungarian | Spanish | German | French | Selection |
| --- | --- | --- | --- | --- | --- |
| WinUI 3 app | complete | complete | complete | translated | translated | Windows UI culture or in-app override |
| Portable egui GUI | complete | complete | complete | translated | translated | Settings picker or `LC_ALL` / `LANG` |
| CLI human output | complete | complete | complete | translated | translated | `SENSITIVITY_LANG`, `LC_ALL`, or `LANG` |
| Inno Setup UI | built-in | built-in | built-in | built-in | built-in | Windows installer language detection |
| Documentation and wiki | English | not translated | not translated | not translated | not translated | Manual translation |

The supported locale set is currently `en`, `hu`, `es`, `de`, and `fr`. A region profile is
not a language. The existing `in`, `ru`, `id`, `tr`, `tw`, and `cn` ROM profiles
do not imply that those UI languages are supported.

## Completed in the current localization pass

The previously missing 22 Hungarian and 1 Spanish Windows source entries are
now translated and present in every catalog. The validator compares the full
English Windows key set and reports 201 complete source entries.

The completed Hungarian entries were:

- `Allowed ROMs`
- `Check allowed ROMs`
- `Checking allowed ROMs…`
- `Choose a verified package`
- `Download and flash latest`
- `Download and flash the latest ROM?`
- `Download latest ROM`
- `Download the current official Recovery ROM, or use an official ZIP you already have.`
- `Downloading the latest official ROM…`
- `Follow these three steps in order. The detailed tools remain available in the navigation.`
- `Get official ROM`
- `Get the current official Recovery ROM for the connected phone, or review the packages accepted by Xiaomi.`
- `Latest official Recovery ROM`
- `No recovery connected`
- `No ROM query has been run yet.`
- `Official ROMs`
- `Only set a profile or codename when device recovery cannot report correct values.`
- `ROM download completed`
- `ROM library`
- `Sensitivity validates the ROM before transfer and asks again if a wipe is required.`
- `Sensitivity will download the official Recovery ROM, verify it, and request another confirmation if Xiaomi requires a data wipe.`
- `The download is verified against Xiaomi's MD5 before it is kept.`

Spanish `No recovery connected` was also added. The locale validator now fails
on any full source-key difference, not only on semantic alias differences.

German (`de`) and French (`fr`) were added across CLI, portable GUI, WinUI, and
the Inno Setup language list. The runtime catalogs have the same key sets as
English, and the critical recovery, flash, erase, and validation terms were
reviewed after the initial machine translation passes.

## Remaining engineering work

1. Complete CLI error localization. Human status output and the main command
   contexts are catalogued, but lower-level `anyhow` contexts and protocol
   diagnostics in `src/` are still English.
2. Review the remaining WinUI technical placeholder text, especially the
   example codename `garnet`, and decide whether it should remain as an example
   or use a localized placeholder.
3. Keep protocol tokens such as ADB, WinUSB, CNXN, and sideload-host unchanged
   while translating the remaining user-facing diagnostic context.
4. Decide whether documentation and the wiki should be translated. This is a
   separate content task, not a runtime locale task.
5. Add a language metadata manifest so every new locale automatically appears
   in the WinUI and portable GUI selectors instead of requiring enum and XAML
   edits in multiple places.
6. Add locale smoke tests for system detection, explicit override, missing-file
   fallback, placeholders, narrow-window layout, and installer language
   resources.

## Candidate language backlog

Prioritize based on existing Xiaomi region profiles and likely user reach:

### Tier 1

Italian (`it`), Polish (`pl`), Czech (`cs`),
Slovak (`sk`), Romanian (`ro`), Turkish (`tr`), Russian (`ru`), Ukrainian (`uk`),
Portuguese (`pt-BR`, then `pt-PT`).

### Tier 2

Dutch (`nl`), Greek (`el`), Bulgarian (`bg`), Croatian (`hr`), Slovenian (`sl`),
Serbian (`sr`), Swedish (`sv`), Danish (`da`), Finnish (`fi`), Norwegian Bokmål
(`nb`).

### Tier 3

Simplified Chinese (`zh-CN`), Traditional Chinese (`zh-TW`), Japanese (`ja`),
Korean (`ko`), Indonesian (`id`), Vietnamese (`vi`), Thai (`th`), Hindi (`hi`),
Arabic (`ar`), and Hebrew (`he`). These require additional typography and
right-to-left layout review where applicable.

For every new language, add one directory under `locales/<language>/` containing
`cli.json`, `gui.json`, and `windows.json`, then add the installer language only
if an Inno Setup `.isl` resource is available and tested.
