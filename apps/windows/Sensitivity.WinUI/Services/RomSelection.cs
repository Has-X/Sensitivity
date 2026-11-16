namespace Sensitivity.WinUI.Services;

public static class RomSelection
{
    // Discovery is only a convenience, never flash authorization. Do not infer
    // compatibility or exclude projects from their filenames.
    public static bool IsRecoveryCandidate(string filename, string? codename)
    {
        var name = Path.GetFileName(filename);
        return name.EndsWith(".zip", StringComparison.OrdinalIgnoreCase)
            && (string.IsNullOrWhiteSpace(codename) || name.Contains(codename, StringComparison.OrdinalIgnoreCase));
    }

    public static string? ChooseCandidate(IReadOnlyList<string> newestFirst, string? codename)
    {
        var matching = newestFirst.FirstOrDefault(path => IsRecoveryCandidate(path, codename));
        if (!string.IsNullOrWhiteSpace(codename) && matching is not null) return matching;
        return newestFirst.Count == 1 && IsRecoveryCandidate(newestFirst[0], null) ? newestFirst[0] : null;
    }
}
