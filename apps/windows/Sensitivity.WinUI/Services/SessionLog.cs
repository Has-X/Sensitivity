using System.Diagnostics;
using System.Globalization;
using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace Sensitivity.WinUI.Services;

/// <summary>Support timeline with allowlisted metadata only. Never write backend
/// stdout/stderr, command arguments, identities, ROM paths, URLs, or tokens.</summary>
public static class SessionLog
{
    private static readonly object Gate = new();
    private static readonly string Root = Environment.GetEnvironmentVariable("SENSITIVITY_LOG_ROOT")
        ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Sensitivity", "Logs");
    public static string SessionDirectory { get; } = Path.Combine(Root, $"session-{DateTime.UtcNow:yyyyMMdd-HHmmss}-{Guid.NewGuid():N}");
    private static readonly string Timeline = Path.Combine(SessionDirectory, "session.jsonl");
    public static bool IsAvailable { get; private set; }

    static SessionLog()
    {
        try
        {
            Directory.CreateDirectory(SessionDirectory);
            foreach (var directory in new DirectoryInfo(Root).EnumerateDirectories("session-*")
                .Where(d => !d.Attributes.HasFlag(FileAttributes.ReparsePoint) && Regex.IsMatch(d.Name, @"^session-\d{8}-\d{6}-[a-f0-9]{32}$"))
                .OrderByDescending(d => d.CreationTimeUtc).Skip(10))
            {
                try { directory.Delete(true); } catch (IOException) { } catch (UnauthorizedAccessException) { }
            }
            IsAvailable = true;
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
        Record("session.started", new { version = typeof(SessionLog).Assembly.GetName().Version?.ToString(),
            os = Environment.OSVersion.VersionString, arch = RuntimeInformation.ProcessArchitecture.ToString(),
            runtime = Environment.Version.ToString(), language = CultureInfo.CurrentUICulture.Name });
    }

    public static void Record(string name, object? details = null)
    {
        if (!IsAvailable) return;
        lock (Gate)
        {
            try
            {
                if (new FileInfo(Timeline) is { Exists: true, Length: > 2 * 1024 * 1024 }) return;
                File.AppendAllText(Timeline, JsonSerializer.Serialize(new { timestamp = DateTimeOffset.UtcNow,
                    @event = name, details }) + Environment.NewLine, Encoding.UTF8);
            }
            catch (IOException) { IsAvailable = false; }
            catch (UnauthorizedAccessException) { IsAvailable = false; }
        }
    }

    public static string ErrorCategory(string text)
    {
        var http = Regex.Match(text, @"\bHTTP\s+(\d{3})\b", RegexOptions.IgnoreCase);
        if (http.Success) return "http_" + http.Groups[1].Value;
        if (text.Contains("token", StringComparison.OrdinalIgnoreCase)) return "validation_token_missing";
        if (text.Contains("MD5 mismatch", StringComparison.OrdinalIgnoreCase)) return "checksum_mismatch";
        if (text.Contains("Invalid data", StringComparison.OrdinalIgnoreCase)) return "invalid_data";
        if (text.Contains("decrypt", StringComparison.OrdinalIgnoreCase) || text.Contains("Base64", StringComparison.OrdinalIgnoreCase)) return "validation_response_encoding";
        if (text.Contains("timed out", StringComparison.OrdinalIgnoreCase)) return "timeout";
        if (text.Contains("Access denied", StringComparison.OrdinalIgnoreCase) || text.Contains("Claiming interface", StringComparison.OrdinalIgnoreCase)) return "usb_access";
        return "operation_failed";
    }

    public static void Failure(Exception error)
    {
        Record("application.error", new { type = error.GetType().Name, category = ErrorCategory(error.Message),
            hresult = error.HResult, stack = new StackTrace(error, false).GetFrames()
                .Take(12).Select(f => $"{f.GetMethod()?.DeclaringType?.FullName}.{f.GetMethod()?.Name}").ToArray() });
    }

    public static void BackendLine(string operation, string line)
    {
        if (!line.StartsWith('{')) return;
        try
        {
            using var document = JsonDocument.Parse(line);
            var root = document.RootElement;
            if (!root.TryGetProperty("event", out var property) || property.ValueKind != JsonValueKind.String) return;
            var name = property.GetString();
            if (name is not ("status" or "progress" or "downloaded" or "confirmation_required" or "completed" or "error")) return;
            if (name == "progress") return; // The core records transfer milestones without flooding the UI timeline.
            Record("backend.event", new { operation, kind = name,
                current = root.TryGetProperty("current", out var current) && current.ValueKind == JsonValueKind.Number && current.TryGetInt64(out var c) ? c : (long?)null,
                total = root.TryGetProperty("total", out var total) && total.ValueKind == JsonValueKind.Number && total.TryGetInt64(out var t) ? t : (long?)null });
        }
        catch (JsonException) { }
    }

    public static void Export(string destination)
    {
        if (!IsAvailable) throw new IOException("Session logging is unavailable");
        Record("report.exported");
        var temporary = destination + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            ExportArchive(temporary);
            File.Move(temporary, destination, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }

    public static string ReadCurrent()
    {
        var text = new StringBuilder();
        foreach (var path in Directory.EnumerateFiles(SessionDirectory, "*.jsonl").OrderBy(p => p))
        {
            using var source = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var reader = new StreamReader(source);
            text.AppendLine(Path.GetFileName(path));
            while (reader.ReadLine() is { } line)
            {
                text.AppendLine(line);
                if (text.Length > 150_000) return text.ToString();
            }
        }
        return text.ToString();
    }

    private static void ExportArchive(string destination)
    {
        using var archive = ZipFile.Open(destination, ZipArchiveMode.Create);
        var readme = archive.CreateEntry("README.txt");
        using (var writer = new StreamWriter(readme.Open()))
            writer.Write("Sensitivity support report\nContains the current and up to two previous session timelines, backend stages, durations, HTTP status, validation response shape, and application stack frames.\nExcludes device serials, validation tokens, ROM paths, signed URLs, raw protocol payloads, settings, and environment variables.\nDescribe what you clicked, what you expected, and what happened when attaching this ZIP to a bug report.\n");
        long remaining = 25 * 1024 * 1024;
        foreach (var directory in new DirectoryInfo(Root).EnumerateDirectories("session-*")
            .Where(d => !d.Attributes.HasFlag(FileAttributes.ReparsePoint))
            .OrderByDescending(d => d.CreationTimeUtc).Take(3))
        foreach (var file in directory.EnumerateFiles("*.jsonl").OrderByDescending(f => f.LastWriteTimeUtc))
        {
            if (file.Attributes.HasFlag(FileAttributes.ReparsePoint) || file.Length > remaining) continue;
            using var source = new FileStream(file.FullName, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            using var target = archive.CreateEntry($"{directory.Name}/{file.Name}", CompressionLevel.Optimal).Open();
            // A running backend may append while exporting. Copy only the snapshot length.
            var length = Math.Min(source.Length, remaining);
            var buffer = new byte[8192];
            while (length > 0)
            {
                var count = source.Read(buffer, 0, (int)Math.Min(buffer.Length, length));
                if (count == 0) break;
                target.Write(buffer, 0, count);
                length -= count;
                remaining -= count;
            }
        }
    }
}
