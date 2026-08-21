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

    private static string L(string key) => LocalizationService.Get(key);

    private static string L(string key, params (string Name, string Value)[] values)
    {
        var text = LocalizationService.Get(key);
        foreach (var (name, value) in values) text = text.Replace($"{{{name}}}", value, StringComparison.Ordinal);
        return text;
    }

    public MainWindow()
    {
        InitializeComponent();
        var settings = SettingsStore.Load();
        LocalizationService.Initialize(settings.LanguageOverride);
        ApplyLocalization();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        SystemBackdrop = new MicaBackdrop { Kind = MicaKind.BaseAlt };
        AppWindow.Resize(new SizeInt32(1120, 760));
        AutoResolveAdbToggle.IsOn = settings.OfferAdbResolution;
        StopAdbToggle.IsOn = settings.AlwaysStopAdb;
        LanguagePicker.SelectedItem = LanguagePicker.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(item => (item.Tag as string ?? string.Empty) == (settings.LanguageOverride ?? string.Empty));
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
                LastRomPath = _romPath,
                LanguageOverride = LocalizationService.OverrideLanguage
            });
        };
    }

    private async void Root_Loaded(object sender, RoutedEventArgs e)
    {
        ApplyLocalization();
        if (!_backend.IsAvailable)
        {
            ShowStatus(L("error.backend_missing"), L("error.backend_missing_detail"), InfoBarSeverity.Error);
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

    private void LanguagePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (LanguagePicker.SelectedItem is not ComboBoxItem item || item.Tag is not string language) return;
        LocalizationService.Initialize(string.IsNullOrWhiteSpace(language) ? null : language);
        ApplyLocalization();
        LanguagePicker.SelectedItem = LanguagePicker.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(candidate => (candidate.Tag as string ?? string.Empty) == (LocalizationService.OverrideLanguage ?? string.Empty));
    }

    private void ApplyLocalization()
    {
        LocalizationService.Apply(Root);
        AboutVersionText.Text = $"{L("app.title")} {typeof(App).Assembly.GetName().Version?.ToString(3)}";
        Title = L("app.title");
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
                0 => L("connection.none"),
                1 => L("connection.found"),
                _ => L("connection.count", ("count", devices.Count.ToString()))
            };
            ConnectionSubtitle.Text = devices.Count == 0
                ? L("connection.phone_hint")
                : L("connection.select_hint");
            if (devices.Count == 0)
            {
                ShowStatus(L("error.no_recovery"), L("error.no_recovery_detail"), InfoBarSeverity.Warning);
            }
            else
            {
                StatusBar.IsOpen = false;
            }
        }, L("status.refreshing"));
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
            ShowStatus(L("dialog.select_recovery"), L("connection.refresh_hint"), InfoBarSeverity.Warning);
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
                    L("dialog.adb_owns"),
                    L("dialog.adb_owns_detail"),
                    L("action.stop_adb_retry"),
                    L("action.keep_adb"));
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
            ConnectionTitle.Text = L("connection.ready", ("device", info.Device));
            ConnectionSubtitle.Text = L("connection.details", ("version", info.Version), ("region", info.Region), ("romzone", info.RomZone)).Trim();
            ShowStatus(L("status.recovery_ready"), L("status.handshake_ok"), InfoBarSeverity.Success);
        }, L("status.reading_recovery"));
    }

    private void GoToFlash_Click(object sender, RoutedEventArgs e)
    {
        Navigation.SelectedItem = Navigation.MenuItems[1];
    }

    private async void BrowseRomButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileOpenPicker
        {
            SuggestedStartLocation = PickerLocationId.Downloads,
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
        OperationStatusText.Text = L("status.ready_to_flash");
    }

    private async void StartFlashButton_Click(object sender, RoutedEventArgs e)
    {
        if (_romPath is null || !File.Exists(_romPath))
        {
            ShowStatus(L("dialog.choose_rom"), L("dialog.choose_rom_detail"), InfoBarSeverity.Warning);
            return;
        }
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        if (!await ShowChoiceAsync(
            L("dialog.validate_flash"),
            L("dialog.validate_flash_detail"),
            L("action.validate_flash"),
            L("action.cancel")))
        {
            return;
        }

        _operationCancellation = new CancellationTokenSource();
        SetBusy(true, L("status.preparing_flash"));
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
            OperationStatusText.Text = L("status.flash_completed");
            ShowStatus(L("status.flash_completed"), L("status.transfer_ok"), InfoBarSeverity.Success);
        }
        catch (OperationCanceledException)
        {
            OperationStatusText.Text = L("status.cancelled");
            ShowStatus(L("status.flash_cancelled"), L("status.cancelled_detail"), InfoBarSeverity.Warning);
        }
        catch (Exception error)
        {
            OperationStatusText.Text = L("status.flash_stopped");
            ShowStatus(L("status.flash_stopped"), CleanError(error.Message), InfoBarSeverity.Error);
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
        return RunOnUiAsync<bool?>(async () =>
        {
            switch (backendEvent.Event)
            {
                case "status":
                    OperationStatusText.Text = backendEvent.Message ?? L("status.working");
                    break;
                case "progress" when backendEvent.Total > 0:
                    var percent = Math.Clamp(backendEvent.Current * 100d / backendEvent.Total, 0, 100);
                    FlashProgress.Value = percent;
                    ProgressPercentText.Text = $"{percent:0}%";
                    break;
                case "completed":
                    OperationStatusText.Text = backendEvent.Message ?? L("status.completed");
                    break;
                case "confirmation_required" when backendEvent.Kind == "data_wipe":
                    return await ShowChoiceAsync(
                        L("dialog.wipe_required"),
                        backendEvent.Message ?? L("dialog.wipe_required_detail"),
                        L("action.erase_continue"),
                        L("action.cancel_flash"));
                case "error":
                    OperationStatusText.Text = backendEvent.Message ?? L("status.operation_failed");
                    break;
            }
            return null;
        });
    }

    private void CancelFlashButton_Click(object sender, RoutedEventArgs e)
    {
        OperationStatusText.Text = L("status.cancel_request");
        CancelFlashButton.IsEnabled = false;
        _operationCancellation?.Cancel();
    }

    private async void RebootButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.RebootAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            ShowStatus(L("status.reboot_requested"), L("status.reboot_detail"), InfoBarSeverity.Success);
        }, L("status.sending_reboot"));
    }

    private async void EraseDataButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        if (!await ShowChoiceAsync(
            L("dialog.erase_confirm"),
            L("dialog.erase_confirm_detail"),
            L("action.erase_data"),
            L("action.cancel")))
        {
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.FormatDataAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            ShowStatus(L("status.erase_requested"), L("status.erase_detail"), InfoBarSeverity.Success);
        }, L("status.erasing"));
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
                result.Succeeded ? L("status.diagnostics_passed") : L("status.diagnostics_problem"),
                result.Succeeded ? L("status.diagnostics_ready") : L("status.diagnostics_review"),
                result.Succeeded ? InfoBarSeverity.Success : InfoBarSeverity.Warning);
        }, L("status.running_diagnostics"));
    }

    private void CopyDiagnosticsButton_Click(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(DiagnosticsText.Text)) return;
        var package = new DataPackage();
        package.SetText(DiagnosticsText.Text);
        Clipboard.SetContent(package);
        ShowStatus(L("status.report_copied"), L("status.report_copied_detail"), InfoBarSeverity.Success);
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
            ShowStatus(L("status.operation_failed"), CleanError(error.Message), InfoBarSeverity.Error);
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
        TitleRefreshButton.IsEnabled = !busy;
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
            completion.SetException(new InvalidOperationException(L("error.window_closing")));
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
            return L("error.no_interface_detail");
        }
        if (cleaned.Contains("Claiming interface", StringComparison.OrdinalIgnoreCase)
            || cleaned.Contains("Opening USB device", StringComparison.OrdinalIgnoreCase))
        {
            return L("error.usb_open_detail");
        }
        if (cleaned.Contains("Validation HTTP", StringComparison.OrdinalIgnoreCase)
            || cleaned.Contains("HTTP request failed", StringComparison.OrdinalIgnoreCase))
        {
            return L("error.validation_http_detail");
        }
        return cleaned;
    }
}
