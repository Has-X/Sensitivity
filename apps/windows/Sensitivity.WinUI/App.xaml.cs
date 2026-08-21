using Microsoft.UI.Xaml;
using Microsoft.UI;
using Microsoft.UI.Xaml.Media;
using Windows.UI.ViewManagement;

namespace Sensitivity.WinUI;

public partial class App : Application
{
    private static readonly Windows.UI.Color XiaomiOrange = ColorHelper.FromArgb(255, 255, 105, 0);
    private readonly UISettings _uiSettings = new();
    private bool _useXiaomiAccent;

    private Window? _window;

    public App()
    {
        InitializeComponent();
        _uiSettings.ColorValuesChanged += (_, _) =>
        {
            if (!_useXiaomiAccent)
            {
                _window?.DispatcherQueue.TryEnqueue(() =>
                    SetThemeBrushColor("SensitivityAccentBrush", _uiSettings.GetColorValue(UIColorType.Accent)));
            }
        };
        UnhandledException += (_, args) =>
        {
            System.Diagnostics.Debug.WriteLine(args.Exception);
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Activate();
    }

    public void SetXiaomiAccent(bool enabled)
    {
        _useXiaomiAccent = enabled;
        var color = enabled ? XiaomiOrange : _uiSettings.GetColorValue(UIColorType.Accent);
        SetThemeBrushColor("SensitivityAccentBrush", color);
    }

    private void SetThemeBrushColor(string key, Windows.UI.Color color)
    {
        foreach (var themeName in new[] { "Light", "Dark" })
        {
            if (Resources.ThemeDictionaries[themeName] is ResourceDictionary theme
                && theme[key] is SolidColorBrush brush)
            {
                brush.Color = color;
            }
        }
    }
}
