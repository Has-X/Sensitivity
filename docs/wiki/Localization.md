# Languages and translation safety

Sensitivity has runtime catalogs for 34 languages across the Windows app,
portable GUI, and CLI. The language picker and system-language detection use
the same locale set. Region profiles such as `eea`, `cn`, and `tr` control ROM
matching, not the interface language.

Product and protocol names stay unchanged: `Sensitivity`, `Chromatic`, Xiaomi,
`Connect with Mi Assistant`, ADB, WinUSB, ROM, ZIP, MD5, and Fastboot. In
particular, **flash** means installing an approved Recovery ROM. It must not be
translated as a camera flash or visual blink.

Every locale is checked for missing keys, placeholder mismatches, empty strings,
and known translation artifacts by:

```powershell
pwsh -NoProfile -File tools/check-locales.ps1
```

Machine-assisted translations provide broad coverage, but native review remains
important for destructive actions and safety messages. A questionable safety
string should use clear English temporarily rather than an ambiguous local term.
