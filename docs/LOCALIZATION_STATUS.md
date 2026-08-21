# Localization status

Status date: 2026-08-22

## Current support

| Surface | English | Hungarian | Spanish | Selection |
| --- | --- | --- | --- | --- |
| WinUI 3 app | complete | 22 source entries missing | 1 source entry missing | Windows UI culture or in-app override |
| Portable egui GUI | complete | complete | complete | Settings picker or `LC_ALL` / `LANG` |
| CLI human output | complete | complete | complete | `SENSITIVITY_LANG`, `LC_ALL`, or `LANG` |
| Inno Setup UI | built-in | built-in | built-in | Windows installer language detection |
| Documentation and wiki | English | not translated | not translated | Manual translation |

The supported locale set is currently `en`, `hu`, and `es`. A region profile is
not a language. The existing `in`, `ru`, `id`, `tr`, `tw`, and `cn` ROM profiles
do not imply that those UI languages are supported.

## Immediate completion work

The English Windows catalog has 22 entries that are not yet present in the
Hungarian catalog:

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

Spanish is missing only `No recovery connected` from the English Windows
source. After these entries are added, the locale validator should compare the
full English key set, not only the semantic alias source keys.

## Remaining engineering work

1. Finish the 23 Windows translations above and strengthen
   `tools/check-locales.ps1` to fail on any source-key difference.
2. Move remaining user-facing WinUI literals into catalogs: region profile
   labels, the `garnet` placeholder where appropriate, backend startup errors,
   and the recovery device display label.
3. Complete CLI error localization. Human status output is catalogued, but
   `anyhow` contexts and protocol diagnostics in `src/` are still English.
   Protocol tokens such as ADB, WinUSB, CNXN, and sideload-host must remain
   unchanged.
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

German (`de`), French (`fr`), Italian (`it`), Polish (`pl`), Czech (`cs`),
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
