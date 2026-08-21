using Microsoft.UI.Xaml;
using Microsoft.UI;
using Sensitivity.WinUI.Services;

namespace Sensitivity.WinUI;

public partial class App : Application
{
    private static readonly Windows.UI.Color XiaomiOrange = ColorHelper.FromArgb(255, 255, 105, 0);
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
        if (enabled) Resources["SystemAccentColor"] = XiaomiOrange;
        else Resources.Remove("SystemAccentColor");
    }

    public void ApplyXiaomiAccent(bool enabled)
    {
        SetXiaomiAccent(enabled);
        var previousWindow = _window;
        if (previousWindow is null) return;

        previousWindow.DispatcherQueue.TryEnqueue(() =>
        {
            if (!ReferenceEquals(_window, previousWindow)) return;
            _window = null;
            previousWindow.Close();
            var refreshedWindow = new MainWindow();
            _window = refreshedWindow;
            refreshedWindow.Activate();
        });
    }

}
