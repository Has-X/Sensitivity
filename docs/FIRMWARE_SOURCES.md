# Firmware catalog sources

Checked 2026-11-16. Third-party indexes provide discovery metadata and extra
download URLs. The shared Rust core still controls integrity checks, Xiaomi
approval, wipe confirmation, and sideload.

| Source | Integration | Evidence and limits |
| --- | --- | --- |
| [XM Firmware Updater](https://github.com/XiaomiFirmwareUpdater/miui-updates-tracker#public-yaml-and-rss) | Active RSS adapter | Publishes reusable per-device feeds with attribution requested. Stable full Recovery entries include official links and full MD5s. The live garnet/TW feed returned one entry and lagged miroms for the newest version. |
| [miroms / hub.miuier.com](https://api.miuier.com/) | Active JSON adapter | Documented V3 API with device branches, regions, versions, and Recovery filenames. The live garnet/TW query returned 24 entries. Checked records lack full MD5s; the ten-character filename suffix is never treated as one. |
| [MiFirm](https://mifirm.net/apisystem) | Deferred | API System currently redirects to login. No account, credentials, or undocumented authenticated endpoints are used. Other package categories would need filtering. |
| [XiaomiROM](https://xiaomirom.com/en/) | Manual reference | Official-ROM catalog; a supported public machine-readable interface was not established. No HTML scraper is shipped. |
| [MIUI ROM](https://miuirom.org/) | Manual reference | Lists Recovery, Fastboot, and OTA packages; no documented public API was verified. No scraper is shipped. |
| [XiaomiFirmware.com](https://xiaomifirmware.com/) | Manual reference | Mixes ROM pages, tools, and custom-ROM material. No documented machine-readable full-Recovery interface was verified. |

These observations are not permanent availability guarantees. Credit for XFU
feed data goes to XiaomiFirmwareUpdater and yshalsager. Credit for miroms data
goes to [HegeKen/miroms](https://github.com/HegeKen/miroms). These projects are
independent of Sensitivity. No catalog data or upstream application code is
bundled into the installer.

## Use

In the Windows ROMs page, **Search firmware catalogs** uses the detected
codename or the codename entered in Settings. A selected region profile or
recognized detected region narrows the result. A disconnected phone is
supported when the codename is entered manually. Results show source, region,
version, filename, URL, optional MD5, and source availability. Copy a desired
download link to obtain the package manually, then use the normal ROM selection
and Xiaomi-validation flow.

Without connecting a phone or changing the ADB server:

```console
sensitivity rom-catalog --device garnet --region TW
```

Omit `--region` for all supported regions. The command emits JSON. Aliases such
as `global`, `eea`, and `taiwan` are accepted. One provider's failure does not
hide another provider's results. All providers unavailable means a nonzero
exit code.

## Automatic download fallback

Normal downloads try Xiaomi's advertised URLs and public CDNs first. If these
fail, catalogs supply URLs for the **exact same filename** in the device index.
A catalog MD5, if present, must agree with Xiaomi's MD5. Missing catalog MD5s
never replace or weaken Xiaomi's expected checksum. Candidates must use HTTPS
on explicitly allowed Xiaomi CDN/storage hosts, pass a ZIP-byte probe, and pass
full-file verification against Xiaomi's MD5. Another region or version is never
silently substituted, and a catalog entry cannot authorize flashing.

Catalog discovery includes only stable full Recovery ZIPs. Fastboot archives,
incremental OTAs, standalone firmware/vendor packages, and developer branches
are different package types and are excluded. Local ZIP auto-selection remains
configurable and has no project-name exclusions.

Providers receive only the public device codename, not serials, validation
tokens, local ROM paths, or Xiaomi credentials. Requests are bounded by a
timeout and 4 MiB response limit. Session logs contain source statuses and
counts, not raw catalog bodies or URLs.

Live checks returned HTTP 206 and ZIP headers for a garnet/TW package from
each active provider. This proves availability of package bytes at check time,
not full ROM integrity or a real flash.
