using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Automation;

namespace Sensitivity.WinUI.Services;

public static class LocalizationService
{
    private static readonly string[] SupportedLanguages =
    {
        "en", "hu", "es", "de", "fr", "it", "pl", "pt-BR", "tr", "id", "ro", "cs", "sk", "ru", "uk",
        "zh-CN", "ar", "vi", "th", "hi", "zh-TW", "ja", "ko", "nl", "el", "bg", "hr", "sr", "sl",
        "sv", "da", "fi", "nb", "pt-PT"
    };
    private static IReadOnlyDictionary<string, string> _strings = new Dictionary<string, string>();

    public static string CurrentLanguage { get; private set; } = "en";
    public static string? OverrideLanguage { get; private set; }

    public static void Initialize(string? overrideLanguage)
    {
        OverrideLanguage = string.IsNullOrWhiteSpace(overrideLanguage) ? null : overrideLanguage.Trim();
        var requested = OverrideLanguage is null
            ? CultureInfo.CurrentUICulture.Name
            : OverrideLanguage;
        CurrentLanguage = ResolveLanguage(requested);
        var path = Path.Combine(AppContext.BaseDirectory, "Resources", "locales", CurrentLanguage, "windows.json");
        try
        {
            var catalog = JsonSerializer.Deserialize(File.ReadAllText(path), SensitivityJsonContext.Default.DictionaryStringString)
                ?? new Dictionary<string, string>();
            var aliases = JsonSerializer.Deserialize(
                File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Resources", "locales", "_keys", "windows.json")),
                SensitivityJsonContext.Default.DictionaryStringString)
                ?? new Dictionary<string, string>();
            foreach (var (id, sourceKey) in aliases)
            {
                catalog[id] = catalog.TryGetValue(sourceKey, out var value) ? value : sourceKey;
            }
            _strings = catalog;
        }
        catch
        {
            _strings = new Dictionary<string, string>();
        }
    }

    private static string ResolveLanguage(string requested)
    {
        if (SupportedLanguages.Contains(requested, StringComparer.OrdinalIgnoreCase))
            return SupportedLanguages.First(language => string.Equals(language, requested, StringComparison.OrdinalIgnoreCase));
        var baseLanguage = requested.Split('-', '_')[0].ToLowerInvariant();
        if (baseLanguage == "pt") return "pt-BR";
        if (baseLanguage == "zh") return requested.Contains("TW", StringComparison.OrdinalIgnoreCase) ? "zh-TW" : "zh-CN";
        return SupportedLanguages.FirstOrDefault(language => language.Equals(baseLanguage, StringComparison.OrdinalIgnoreCase)) ?? "en";
    }

    public static string Get(string key) => _strings.TryGetValue(key, out var value) ? value : key;

    public static void Apply(FrameworkElement root)
    {
        ApplyElement(root);
        ApplyChildren(root);
    }

    private static void ApplyChildren(DependencyObject parent)
    {
        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(parent); index++)
        {
            if (VisualTreeHelper.GetChild(parent, index) is FrameworkElement child)
            {
                ApplyElement(child);
                ApplyChildren(child);
            }
        }
    }

    private static void ApplyElement(FrameworkElement element)
    {
        var key = element.Tag as string;
        if (string.IsNullOrWhiteSpace(key) || !_strings.ContainsKey(key))
        {
            var automationKey = AutomationProperties.GetName(element);
            key = !string.IsNullOrWhiteSpace(automationKey) && automationKey.Contains('.')
                ? automationKey
                : null;
        }
        if (string.IsNullOrWhiteSpace(key)) return;
        var value = Get(key);
        switch (element)
        {
            case TextBlock textBlock:
                textBlock.Text = value;
                break;
            case Button button when button.Content is not UIElement:
                button.Content = value;
                break;
            case HyperlinkButton hyperlinkButton:
                hyperlinkButton.Content = value;
                break;
            case NavigationViewItem navigationItem:
                navigationItem.Content = value;
                break;
            case ToggleSwitch toggle:
                toggle.Header = value;
                toggle.OffContent = Get($"{key}.off");
                toggle.OnContent = Get($"{key}.on");
                break;
            case ComboBox comboBox:
                comboBox.Header = value;
                break;
            case ComboBoxItem comboBoxItem:
                comboBoxItem.Content = value;
                break;
            case TextBox textBox:
                textBox.PlaceholderText = value;
                break;
        }
    }
}
