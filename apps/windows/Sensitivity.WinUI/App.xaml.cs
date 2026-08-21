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
        "AccentButtonBorderBrush",
        "ToggleSwitchFillOn",
        "ToggleSwitchStrokeOn"
    };
    private static readonly string[] HoverBrushKeys =
    {
        "AccentButtonBackgroundPointerOver",
        "AccentButtonBorderBrushPointerOver",
        "ToggleSwitchFillOnPointerOver",
        "ToggleSwitchStrokeOnPointerOver"
    };
    private static readonly string[] PressedBrushKeys =
    {
        "AccentButtonBackgroundPressed",
        "AccentButtonBorderBrushPressed",
        "ToggleSwitchFillOnPressed",
        "ToggleSwitchStrokeOnPressed"
    };
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
                _window?.DispatcherQueue.TryEnqueue(() => SetXiaomiAccent(false));
            }
        };
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
        _useXiaomiAccent = enabled;
        var baseColor = enabled ? XiaomiOrange : _uiSettings.GetColorValue(UIColorType.Accent);
        var hoverColor = enabled
            ? ColorHelper.FromArgb(255, 255, 131, 51)
            : _uiSettings.GetColorValue(UIColorType.AccentLight1);
        var pressedColor = enabled
            ? ColorHelper.FromArgb(255, 214, 87, 0)
            : _uiSettings.GetColorValue(UIColorType.AccentDark1);
        SetBrushColors(AccentBrushKeys, baseColor);
        SetBrushColors(HoverBrushKeys, hoverColor);
        SetBrushColors(PressedBrushKeys, pressedColor);
    }

    private void SetBrushColors(IEnumerable<string> keys, Windows.UI.Color color)
    {
        foreach (var key in keys)
        {
            if (Resources[key] is SolidColorBrush brush) brush.Color = color;
        }
    }

}
