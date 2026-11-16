using Microsoft.UI.Xaml;

namespace Sensitivity.WinUI;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        Services.SessionLog.Record("application.starting");
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            Services.SessionLog.Failure(args.Exception);
        };
        AppDomain.CurrentDomain.UnhandledException += (_, args) =>
        {
            if (args.ExceptionObject is Exception error) Services.SessionLog.Failure(error);
        };
        TaskScheduler.UnobservedTaskException += (_, args) => Services.SessionLog.Failure(args.Exception);
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Activate();
    }
}
