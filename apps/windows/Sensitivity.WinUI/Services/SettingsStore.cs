using System.Text.Json;

namespace Sensitivity.WinUI.Services;

public sealed class AppSettings
{
    public bool OfferAdbResolution { get; set; } = true;
    public bool AlwaysStopAdb { get; set; }
    public string? LastRomPath { get; set; }
    public string? DownloadDirectory { get; set; }
    public string? RegionProfile { get; set; }
    public string? Codename { get; set; }
    public string? LanguageOverride { get; set; }
}

public static class SettingsStore
{
    private static readonly string SettingsPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Sensitivity",
        "settings.json");

    public static AppSettings Load()
    {
        try
        {
            return File.Exists(SettingsPath)
                ? JsonSerializer.Deserialize<AppSettings>(File.ReadAllText(SettingsPath)) ?? new AppSettings()
                : new AppSettings();
        }
        catch
        {
            return new AppSettings();
        }
    }

    public static void Save(AppSettings settings)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(SettingsPath)!);
            var temporaryPath = SettingsPath + ".tmp";
            File.WriteAllText(temporaryPath, JsonSerializer.Serialize(settings));
            File.Move(temporaryPath, SettingsPath, true);
        }
        catch
        {
            // A settings failure must never block recovery operations or exit.
        }
    }
}
