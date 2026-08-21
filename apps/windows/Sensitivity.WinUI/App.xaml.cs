using Microsoft.UI.Xaml;
using Microsoft.UI;
using Microsoft.UI.Xaml.Media;

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
        _window = new MainWindow();
        _window.Activate();
    }

    public void SetXiaomiAccent(bool enabled)
    {
        if (enabled)
        {
            Resources["AccentFillColorDefaultBrush"] = new SolidColorBrush(XiaomiOrange);
            Resources["AccentFillColorSecondaryBrush"] = new SolidColorBrush(XiaomiOrange);
            Resources["AccentFillColorTertiaryBrush"] = new SolidColorBrush(XiaomiOrange);
            Resources["AccentContentForegroundBrush"] = new SolidColorBrush(Colors.Black);
            return;
        }

        Resources.Remove("AccentFillColorDefaultBrush");
        Resources.Remove("AccentFillColorSecondaryBrush");
        Resources.Remove("AccentFillColorTertiaryBrush");
        Resources.Remove("AccentContentForegroundBrush");
    }
}
