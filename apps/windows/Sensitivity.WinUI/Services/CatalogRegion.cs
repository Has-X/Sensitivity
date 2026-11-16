namespace Sensitivity.WinUI.Services;

public static class CatalogRegion
{
    public static bool IsKnown(string region) => new[] { "MI", "EU", "IN", "ID", "TW", "TR", "RU", "CN", "JP" }
        .Contains(region, StringComparer.OrdinalIgnoreCase);
}
