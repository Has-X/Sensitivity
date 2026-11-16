using System.IO.Compression;
using System.Text.Json;
using Sensitivity.WinUI.Services;

var root = Path.Combine(Path.GetTempPath(), "Sensitivity-tests-" + Guid.NewGuid().ToString("N"));
Directory.CreateDirectory(root);
Environment.SetEnvironmentVariable("SENSITIVITY_LOG_ROOT", Path.Combine(root, "logs"));
var passed = 0;
void Check(bool condition, string description)
{
    if (!condition) throw new Exception(description);
    passed++;
}

try
{
    var migrated = JsonSerializer.Deserialize("{}", SensitivityJsonContext.Default.AppSettings)!;
    Check(migrated.AutoSelectRom, "Existing settings keep automatic selection enabled by default");
    migrated.AutoSelectRom = false;
    var saved = JsonSerializer.Serialize(migrated, SensitivityJsonContext.Default.AppSettings);
    Check(!JsonSerializer.Deserialize(saved, SensitivityJsonContext.Default.AppSettings)!.AutoSelectRom, "Disabled selection survives settings serialization");
    Check(RomSelection.IsRecoveryCandidate("PBRP-garnet-4.0-20250625-2100-OFFICIAL.zip", "garnet"), "No project-name exclusions");
    Check(RomSelection.IsRecoveryCandidate("twrp-garnet.zip", "garnet"), "Discovery does not authorize flashing");
    Check(!RomSelection.IsRecoveryCandidate("ruby_global-ota_full-version.zip", "garnet"), "Reject other devices");
    Check(RomSelection.IsRecoveryCandidate("garnet_tw_global-ota_full-version.zip", null), "Accept a single ZIP without known identity");
    Check(RomSelection.ChooseCandidate(["one.zip", "two.zip"], null) is null, "Do not guess among unrelated ZIPs");
    Check(RomSelection.ChooseCandidate(["other.zip", "PBRP-garnet.zip"], "garnet") == "PBRP-garnet.zip", "Prefer codename match");
    Check(RomSelection.ChooseCandidate(["one.zip"], "garnet") == "one.zip", "Retain single-ZIP convenience");
    Check(RomSelection.IsRecoveryCandidate("garnet_tw_global-ota_full-OS3.0.303.0.WNRTWXM-user-16.0-checksum.zip", "garnet"), "Find HyperOS recovery package");
    Check(RomSelection.IsRecoveryCandidate("miui_GARNETTWGlobal_OS1.0.1.0.zip", "garnet"), "Find legacy MIUI package");
    var rom = Path.Combine(root, "official.zip");
    File.WriteAllText(rom, "test package");
    var downloaded = JsonSerializer.Serialize(new { @event = "downloaded", path = rom, md5_verified = true });
    Check(DownloadedRomResult.GetVerifiedPath("{\"event\":\"status\"}\n" + downloaded) == rom, "Read newline-delimited verified download result");
    Check(DownloadedRomResult.GetVerifiedPath(JsonSerializer.Serialize(new { @event = "downloaded", path = rom })) is null, "Require checksum proof");
    Check(DownloadedRomResult.GetVerifiedPath("Downloaded to " + rom + " (md5 ok)") is null, "Never infer download success from prose");
    Check(DownloadedRomResult.GetVerifiedPath("{\"event\":{}}\n{\"event\":\"downloaded\",\"path\":9}") is null, "Ignore malformed machine events");

    const string secret = "DO-NOT-EXPORT-PRIVATE-DATA";
    SessionLog.BackendLine("operation-1", JsonSerializer.Serialize(new { @event = "downloaded", path = secret, serial = secret, token = secret, message = secret }));
    SessionLog.BackendLine("operation-1", "{\"event\":{}}");
    SessionLog.BackendLine("operation-1", "{\"event\":\"status\",\"current\":\"bad\"}");
    SessionLog.BackendLine("operation-1", secret);
    SessionLog.Failure(new InvalidOperationException("HTTP 403 from https://private.invalid/?s=" + secret));
    Check(SessionLog.ErrorCategory("HTTP 403 Forbidden") == "http_403", "Preserve actionable HTTP category");
    var timeline = SessionLog.ReadCurrent();
    Check(timeline.Contains("http_403") && timeline.Contains("backend.event"), "Keep diagnostic stages and failure category");
    Check(!timeline.Contains(secret) && !timeline.Contains("private.invalid"), "Never log raw output, identities, tokens, paths, or URLs");
    var report = Path.Combine(root, "report.zip");
    File.WriteAllText(report, "old report");
    SessionLog.Export(report);
    using (var zip = ZipFile.OpenRead(report))
    {
        Check(zip.Entries.Any(e => e.Name == "session.jsonl"), "Report includes current timeline");
        Check(zip.Entries.Any(e => e.Name == "README.txt"), "Report includes reporting instructions");
        foreach (var entry in zip.Entries)
        {
            using var reader = new StreamReader(entry.Open());
            Check(!reader.ReadToEnd().Contains(secret), "Report excludes private values");
        }
    }
    Check(!Directory.EnumerateFiles(root, "*.tmp").Any(), "Export finalizes without orphaned staging files");
    Console.WriteLine($"Windows service regressions passed: {passed} assertions.");
}
finally
{
    Environment.SetEnvironmentVariable("SENSITIVITY_LOG_ROOT", null);
    Directory.Delete(root, true);
}
