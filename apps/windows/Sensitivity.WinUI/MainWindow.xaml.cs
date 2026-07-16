using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Sensitivity.WinUI.Models;
using Sensitivity.WinUI.Services;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Sensitivity.WinUI;

public sealed partial class MainWindow : Window
{
    private readonly SensitivityBackend _backend = new();
    private CancellationTokenSource? _operationCancellation;
    private string? _romPath;
    private bool _busy;

    public MainWindow()
    {
        InitializeComponent();
        Title = "Sensitivity";
        AboutVersionText.Text = $"Sensitivity {typeof(App).Assembly.GetName().Version?.ToString(3)}";
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        SystemBackdrop = new MicaBackdrop { Kind = MicaKind.BaseAlt };
        AppWindow.Resize(new SizeInt32(1120, 760));
        var settings = SettingsStore.Load();
        AutoResolveAdbToggle.IsOn = settings.OfferAdbResolution;
        StopAdbToggle.IsOn = settings.AlwaysStopAdb;
        if (settings.LastRomPath is { } lastRom && File.Exists(lastRom))
        {
            _romPath = lastRom;
            RomPathText.Text = lastRom;
        }
        Navigation.SelectedItem = Navigation.MenuItems[0];
        Root.Loaded += Root_Loaded;
        Closed += (_, _) =>
        {
            _operationCancellation?.Cancel();
            SettingsStore.Save(new AppSettings
            {
                OfferAdbResolution = AutoResolveAdbToggle.IsOn,
                AlwaysStopAdb = StopAdbToggle.IsOn,
                LastRomPath = _romPath
            });
        };
    }

    private async void Root_Loaded(object sender, RoutedEventArgs e)
    {
        if (!_backend.IsAvailable)
        {
            ShowStatus("Backend missing", "sensitivity.exe was not found beside the application.", InfoBarSeverity.Error);
            SetInteractive(false);
            return;
        }
        await RefreshDevicesAsync();
    }

    private void Navigation_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        var tag = args.IsSettingsSelected
            ? "settings"
            : (args.SelectedItemContainer?.Tag?.ToString() ?? "overview");
        OverviewPage.Visibility = tag == "overview" ? Visibility.Visible : Visibility.Collapsed;
        FlashPage.Visibility = tag == "flash" ? Visibility.Visible : Visibility.Collapsed;
        RecoveryPage.Visibility = tag == "recovery" ? Visibility.Visible : Visibility.Collapsed;
        DiagnosticsPage.Visibility = tag == "diagnostics" ? Visibility.Visible : Visibility.Collapsed;
        SettingsPage.Visibility = tag == "settings" ? Visibility.Visible : Visibility.Collapsed;
    }

    private async void RefreshButton_Click(object sender, RoutedEventArgs e) => await RefreshDevicesAsync();

    private async Task RefreshDevicesAsync()
    {
        await RunBusyAsync(async cancellationToken =>
        {
            var devices = await _backend.GetDevicesAsync(cancellationToken);
            DevicePicker.ItemsSource = devices;
            DevicePicker.SelectedIndex = devices.Count > 0 ? 0 : -1;
            ConnectionTitle.Text = devices.Count switch
            {
                0 => "No recovery connected",
                1 => "Recovery interface found",
                _ => $"{devices.Count} recovery interfaces found"
            };
            ConnectionSubtitle.Text = devices.Count == 0
                ? "On the phone, open Connect with Mi Assistant and reconnect USB."
                : "Select the interface, then read its device information.";
            if (devices.Count == 0)
            {
                ShowStatus("No recovery found", "Check recovery mode, the cable, and the WinUSB driver.", InfoBarSeverity.Warning);
            }
            else
            {
                StatusBar.IsOpen = false;
            }
        }, "Refreshing USB devices…");
    }

    private async void DevicePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        ClearDeviceInfo();
        if (DevicePicker.SelectedItem is UsbDevice)
        {
            await ReadDeviceInfoAsync();
        }
    }

    private async void ReadInfoButton_Click(object sender, RoutedEventArgs e) => await ReadDeviceInfoAsync();

    private async Task ReadDeviceInfoAsync()
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus("Select a recovery", "Refresh USB devices and select one first.", InfoBarSeverity.Warning);
            return;
        }

        await RunBusyAsync(async cancellationToken =>
        {
            DeviceInfo info;
            try
            {
                info = await _backend.GetDeviceInfoAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            }
            catch (Exception error) when (!StopAdbToggle.IsOn
                && AutoResolveAdbToggle.IsOn
                && SensitivityBackend.IsUsbOwnershipError(error))
            {
                var retry = await ShowChoiceAsync(
                    "ADB may be using this interface",
                    "Sensitivity can stop the local ADB server once and retry. Other Android debugging sessions on this computer will disconnect.",
                    "Stop ADB and retry",
                    "Keep it running");
                if (!retry)
                {
                    throw;
                }
                info = await _backend.GetDeviceInfoAsync(device.Index, true, cancellationToken);
            }

            DeviceNameText.Text = info.Device;
            VersionText.Text = info.Version;
            RegionText.Text = string.IsNullOrWhiteSpace(info.Region) ? info.RomZone : info.Region;
            SerialText.Text = info.Serial;
            ConnectionTitle.Text = $"{info.Device} is ready";
            ConnectionSubtitle.Text = $"Recovery {info.Version} · {info.Region} {info.RomZone}".Trim();
            ShowStatus("Recovery ready", "The direct USB handshake and device queries succeeded.", InfoBarSeverity.Success);
        }, "Reading recovery information…");
    }

    private void GoToFlash_Click(object sender, RoutedEventArgs e)
    {
        Navigation.SelectedItem = Navigation.MenuItems[1];
    }

    private async void BrowseRomButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileOpenPicker
        {
            SuggestedStartLocation = PickerLocationId.DownloadsView,
            ViewMode = PickerViewMode.List
        };
        picker.FileTypeFilter.Add(".zip");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        if (file is null)
        {
            return;
        }
        _romPath = file.Path;
        RomPathText.Text = file.Path;
        FlashProgress.Value = 0;
        ProgressPercentText.Text = string.Empty;
        OperationStatusText.Text = "Ready to validate and flash";
    }

    private async void StartFlashButton_Click(object sender, RoutedEventArgs e)
    {
        if (_romPath is null || !File.Exists(_romPath))
        {
            ShowStatus("Choose a ROM", "Select an official Recovery ROM ZIP before continuing.", InfoBarSeverity.Warning);
            return;
        }
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus("No recovery selected", "Connect and select a recovery device first.", InfoBarSeverity.Warning);
            return;
        }
        if (!await ShowChoiceAsync(
            "Validate and flash this ROM?",
            "Sensitivity will verify the package with Xiaomi before starting the transfer. You will get another warning if validation requires a data wipe.",
            "Validate and flash",
            "Cancel"))
        {
            return;
        }

        _operationCancellation = new CancellationTokenSource();
        SetBusy(true, "Preparing the flash…");
        FlashProgress.Value = 0;
        try
        {
            var result = await _backend.FlashAsync(
                device.Index,
                _romPath,
                StopAdbToggle.IsOn,
                HandleBackendEventAsync,
                _operationCancellation.Token);
            if (_operationCancellation.IsCancellationRequested)
            {
                throw new OperationCanceledException();
            }
            if (!result.Succeeded)
            {
                throw new InvalidOperationException(result.ErrorMessage);
            }
            FlashProgress.Value = 100;
            ProgressPercentText.Text = "100%";
            OperationStatusText.Text = "Flash completed";
            ShowStatus("Flash completed", "Recovery accepted the complete ROM transfer.", InfoBarSeverity.Success);
        }
        catch (OperationCanceledException)
        {
            OperationStatusText.Text = "Cancelled safely";
            ShowStatus("Flash cancelled", "Sensitivity requested a graceful close of the USB session.", InfoBarSeverity.Warning);
        }
        catch (Exception error)
        {
            OperationStatusText.Text = "Flash stopped";
            ShowStatus("Flash failed", CleanError(error.Message), InfoBarSeverity.Error);
        }
        finally
        {
            _operationCancellation.Dispose();
            _operationCancellation = null;
            SetBusy(false);
        }
    }

    private Task<bool?> HandleBackendEventAsync(BackendEvent backendEvent)
    {
        return RunOnUiAsync(async () =>
        {
            switch (backendEvent.Event)
            {
                case "status":
                    OperationStatusText.Text = backendEvent.Message ?? "Working…";
                    break;
                case "progress" when backendEvent.Total > 0:
                    var percent = Math.Clamp(backendEvent.Current * 100d / backendEvent.Total, 0, 100);
                    FlashProgress.Value = percent;
                    ProgressPercentText.Text = $"{percent:0}%";
                    break;
                case "completed":
                    OperationStatusText.Text = backendEvent.Message ?? "Completed";
                    break;
                case "confirmation_required" when backendEvent.Kind == "data_wipe":
                    return await ShowChoiceAsync(
                        "This flash will erase all user data",
                        backendEvent.Message ?? "Xiaomi requires a permanent data wipe for this package.",
                        "Erase data and continue",
                        "Cancel flash");
                case "error":
                    OperationStatusText.Text = backendEvent.Message ?? "Operation failed";
                    break;
            }
            return null;
        });
    }

    private void CancelFlashButton_Click(object sender, RoutedEventArgs e)
    {
        OperationStatusText.Text = "Requesting safe cancellation…";
        CancelFlashButton.IsEnabled = false;
        _operationCancellation?.Cancel();
    }

    private async void RebootButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus("No recovery selected", "Connect and select a recovery device first.", InfoBarSeverity.Warning);
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.RebootAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            ShowStatus("Reboot requested", "The phone should leave recovery shortly.", InfoBarSeverity.Success);
        }, "Sending reboot command…");
    }

    private async void EraseDataButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus("No recovery selected", "Connect and select a recovery device first.", InfoBarSeverity.Warning);
            return;
        }
        if (!await ShowChoiceAsync(
            "Permanently erase all user data?",
            "This cannot be undone. Sensitivity will format the phone and request a reboot.",
            "Erase all data",
            "Cancel"))
        {
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.FormatDataAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            ShowStatus("Erase requested", "Recovery accepted the format and reboot commands.", InfoBarSeverity.Success);
        }, "Erasing user data…");
    }

    private async void RunDoctorButton_Click(object sender, RoutedEventArgs e)
    {
        var index = (DevicePicker.SelectedItem as UsbDevice)?.Index ?? 0;
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.RunDoctorAsync(index, StopAdbToggle.IsOn, cancellationToken);
            DiagnosticsText.Text = string.Join(
                Environment.NewLine,
                new[] { result.StandardOutput.Trim(), result.StandardError.Trim() }
                    .Where(value => !string.IsNullOrWhiteSpace(value)));
            ShowStatus(
                result.Succeeded ? "Diagnostics passed" : "Diagnostics found a problem",
                result.Succeeded ? "USB and recovery communication are ready." : "Review the report for the corrective action.",
                result.Succeeded ? InfoBarSeverity.Success : InfoBarSeverity.Warning);
        }, "Running diagnostics…");
    }

    private void CopyDiagnosticsButton_Click(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(DiagnosticsText.Text)) return;
        var package = new DataPackage();
        package.SetText(DiagnosticsText.Text);
        Clipboard.SetContent(package);
        ShowStatus("Report copied", "The diagnostic report is on the clipboard.", InfoBarSeverity.Success);
    }

    private async Task RunBusyAsync(Func<CancellationToken, Task> operation, string status)
    {
        if (_busy) return;
        using var cancellation = new CancellationTokenSource();
        SetBusy(true, status);
        try
        {
            await operation(cancellation.Token);
        }
        catch (Exception error)
        {
            ShowStatus("Operation failed", CleanError(error.Message), InfoBarSeverity.Error);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void SetBusy(bool busy, string? status = null)
    {
        _busy = busy;
        BusyRing.IsActive = busy;
        RefreshButton.IsEnabled = !busy;
        ReadInfoButton.IsEnabled = !busy;
        StartFlashButton.IsEnabled = !busy;
        CancelFlashButton.IsEnabled = busy && _operationCancellation is not null;
        DevicePicker.IsEnabled = !busy;
        if (status is not null) OperationStatusText.Text = status;
    }

    private void SetInteractive(bool enabled)
    {
        Navigation.IsEnabled = enabled;
        RefreshButton.IsEnabled = enabled;
    }

    private void ClearDeviceInfo()
    {
        DeviceNameText.Text = "—";
        VersionText.Text = "—";
        RegionText.Text = "—";
        SerialText.Text = "—";
    }

    private void ShowStatus(string title, string message, InfoBarSeverity severity)
    {
        StatusBar.Title = title;
        StatusBar.Message = message;
        StatusBar.Severity = severity;
        StatusBar.IsOpen = true;
    }

    private async Task<bool> ShowChoiceAsync(
        string title,
        string message,
        string primaryText,
        string closeText)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = Root.XamlRoot,
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap, MaxWidth = 480 },
            PrimaryButtonText = primaryText,
            CloseButtonText = closeText,
            DefaultButton = ContentDialogButton.Close
        };
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private Task<T> RunOnUiAsync<T>(Func<Task<T>> action)
    {
        var completion = new TaskCompletionSource<T>();
        if (!DispatcherQueue.TryEnqueue(async () =>
        {
            try { completion.SetResult(await action()); }
            catch (Exception error) { completion.SetException(error); }
        }))
        {
            completion.SetException(new InvalidOperationException("The application window is closing."));
        }
        return completion.Task;
    }

    private static string CleanError(string message)
    {
        const string prefix = "Error: ";
        var cleaned = message.Trim();
        cleaned = cleaned.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)
            ? cleaned[prefix.Length..]
            : cleaned;
        if (cleaned.Contains("No Mi Assistant ADB interface", StringComparison.OrdinalIgnoreCase))
        {
            return "No Mi Assistant recovery interface was found. Reconnect the phone after opening Connect with Mi Assistant.";
        }
        if (cleaned.Contains("Claiming interface", StringComparison.OrdinalIgnoreCase)
            || cleaned.Contains("Opening USB device", StringComparison.OrdinalIgnoreCase))
        {
            return "Windows could not open the recovery interface. Check WinUSB setup, close other phone tools, or allow Sensitivity to stop local ADB.";
        }
        if (cleaned.Contains("Validation HTTP", StringComparison.OrdinalIgnoreCase)
            || cleaned.Contains("HTTP request failed", StringComparison.OrdinalIgnoreCase))
        {
            return "Xiaomi validation could not be reached. Check the internet connection and try again without changing the ROM.";
        }
        return cleaned;
    }
}
