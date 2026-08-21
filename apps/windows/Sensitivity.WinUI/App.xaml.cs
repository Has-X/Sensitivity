using Microsoft.UI.Xaml;
using Microsoft.UI;
using Microsoft.UI.Xaml.Media;
using Sensitivity.WinUI.Services;
using Windows.UI.ViewManagement;

namespace Sensitivity.WinUI;

public partial class App : Application
{
    private static readonly Windows.UI.Color XiaomiOrange = ColorHelper.FromArgb(255, 255, 105, 0);
    private static readonly string[] AccentBrushKeys =
    {
        "SensitivityAccentBrush",
        "AccentButtonBackground",
        "AccentButtonBackgroundPointerOver",
        "AccentButtonBackgroundPressed",
        "ToggleSwitchFillOn",
        "ToggleSwitchFillOnPointerOver",
        "ToggleSwitchFillOnPressed",
        "ToggleSwitchStrokeOn",
        "ToggleSwitchStrokeOnPointerOver",
        "ToggleSwitchStrokeOnPressed"
    };
    private readonly UISettings _uiSettings = new();
    private Window? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            System.Diagnostics.Debug.WriteLine(args.Exception);
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        SetXiaomiAccent(SettingsStore.Load().UseXiaomiAccent);
        _window = new MainWindow();
        _window.Activate();
    }

    public void SetXiaomiAccent(bool enabled)
    {
        var color = enabled ? XiaomiOrange : _uiSettings.GetColorValue(UIColorType.Accent);
        foreach (var key in AccentBrushKeys)
        {
            if (Resources[key] is SolidColorBrush brush) brush.Color = color;
        }
    }

}
