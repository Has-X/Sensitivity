using System.Text.Json;

namespace Sensitivity.WinUI.Services;

public static class DownloadedRomResult
{
    public static string? GetVerifiedPath(string output)
    {
        foreach (var line in output.Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries))
        {
            try
            {
                using var document = JsonDocument.Parse(line);
                var root = document.RootElement;
                if (root.ValueKind != JsonValueKind.Object
                    || !root.TryGetProperty("event", out var name) || name.ValueKind != JsonValueKind.String || name.GetString() != "downloaded"
                    || !root.TryGetProperty("md5_verified", out var verified) || verified.ValueKind != JsonValueKind.True
                    || !root.TryGetProperty("path", out var path) || path.ValueKind != JsonValueKind.String) continue;
                var value = path.GetString();
                if (!string.IsNullOrWhiteSpace(value) && File.Exists(value)) return value;
            }
            catch (JsonException) { }
        }
        return null;
    }
}
