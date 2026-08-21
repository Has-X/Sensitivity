using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
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
    private string? _detectedCodename;
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
        AppWindow.TitleBar.PreferredHeightOption = TitleBarHeightOption.Tall;
        UpdateTitleBarLayout();
        SystemBackdrop = new MicaBackdrop { Kind = MicaKind.BaseAlt };
        AppWindow.Resize(new SizeInt32(1120, 760));
        AutoResolveAdbToggle.IsOn = settings.OfferAdbResolution;
        StopAdbToggle.IsOn = settings.AlwaysStopAdb;
        _backend.Profile = settings.RegionProfile;
        _backend.Codename = settings.Codename;
        DownloadDirectoryText.Text = string.IsNullOrWhiteSpace(settings.DownloadDirectory)
            ? Environment.GetFolderPath(Environment.SpecialFolder.UserProfile) + "\\Downloads"
            : settings.DownloadDirectory;
        CodenameText.Text = settings.Codename ?? string.Empty;
        RegionProfilePicker.SelectedItem = RegionProfilePicker.Items.OfType<ComboBoxItem>()
            .FirstOrDefault(item => (item.Tag as string ?? string.Empty) == (settings.RegionProfile ?? string.Empty));
        LanguagePicker.SelectedItem = LanguagePicker.Items
            .OfType<ComboBoxItem>()
            .FirstOrDefault(item => (item.Tag as string ?? string.Empty) == (settings.LanguageOverride ?? string.Empty));
        if (settings.LastRomPath is { } lastRom && File.Exists(lastRom))
        {
            SelectRomPath(lastRom);
        }
        else TrySelectDownloadedRom();
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
                DownloadDirectory = DownloadDirectoryText.Text,
                RegionProfile = _backend.Profile,
                Codename = _backend.Codename,
                LanguageOverride = LocalizationService.OverrideLanguage
            });
        };
    }

    private async void Root_Loaded(object sender, RoutedEventArgs e)
    {
        UpdateTitleBarLayout();
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
            : ((args.SelectedItem as NavigationViewItem)?.Tag?.ToString()
                ?? args.SelectedItemContainer?.Tag?.ToString()
                ?? "overview");
        OverviewPage.Visibility = tag == "overview" ? Visibility.Visible : Visibility.Collapsed;
        FlashPage.Visibility = tag == "flash" ? Visibility.Visible : Visibility.Collapsed;
        RomsPage.Visibility = tag == "roms" ? Visibility.Visible : Visibility.Collapsed;
        RecoveryPage.Visibility = tag == "recovery" ? Visibility.Visible : Visibility.Collapsed;
        DiagnosticsPage.Visibility = tag == "diagnostics" ? Visibility.Visible : Visibility.Collapsed;
        SettingsPage.Visibility = tag == "settings" ? Visibility.Visible : Visibility.Collapsed;
        var page = tag switch
        {
            "flash" => FlashPage,
            "roms" => RomsPage,
            "recovery" => RecoveryPage,
            "diagnostics" => DiagnosticsPage,
            "settings" => SettingsPage,
            _ => OverviewPage
        };
        ApplyVisiblePageLocalization(page);
    }

    private static void ApplyVisiblePageLocalization(FrameworkElement page)
    {
        if (page.Visibility != Visibility.Visible) return;
        page.UpdateLayout();
        LocalizationService.Apply(page);
    }

    private void UpdateTitleBarLayout()
    {
        var rightInset = AppWindow.TitleBar.RightInset;
        AppTitleBar.Padding = new Thickness(16, 0, Math.Max(16, rightInset + 8), 0);
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
        LocalizationService.Apply(OverviewPage);
        LocalizationService.Apply(FlashPage);
        LocalizationService.Apply(RomsPage);
        LocalizationService.Apply(RecoveryPage);
        LocalizationService.Apply(DiagnosticsPage);
        LocalizationService.Apply(SettingsPage);
        LanguagePicker.Header = L("label.language");
        foreach (var item in LanguagePicker.Items.OfType<ComboBoxItem>())
        {
            item.Content = L(AutomationProperties.GetName(item));
        }
        foreach (var item in RegionProfilePicker.Items.OfType<ComboBoxItem>())
        {
            var localizationKey = AutomationProperties.GetName(item);
            if (!string.IsNullOrWhiteSpace(localizationKey)) item.Content = L(localizationKey);
        }
        AboutVersionText.Text = $"{L("app.title")} {typeof(App).Assembly.GetName().Version?.ToString(3)}";
        Title = L("app.title");
    }

    private void RegionProfilePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        _backend.Profile = (RegionProfilePicker.SelectedItem as ComboBoxItem)?.Tag as string;
        if (string.IsNullOrWhiteSpace(_backend.Profile)) _backend.Profile = null;
    }

    private void CodenameText_TextChanged(object sender, TextChangedEventArgs e)
        => _backend.Codename = string.IsNullOrWhiteSpace(CodenameText.Text) ? null : CodenameText.Text.Trim();

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
            _detectedCodename = DeriveCodename(info.Device);
            VersionText.Text = info.Version;
            RegionText.Text = string.IsNullOrWhiteSpace(info.Region) ? info.RomZone : info.Region;
            SerialText.Text = info.Serial;
            ConnectionTitle.Text = L("connection.ready", ("device", info.Device));
            ConnectionSubtitle.Text = L("connection.details", ("version", info.Version), ("region", info.Region), ("romzone", info.RomZone)).Trim();
            TrySelectDownloadedRom();
            ShowStatus(L("status.recovery_ready"), L("status.handshake_ok"), InfoBarSeverity.Success);
        }, L("status.reading_recovery"));
    }

    private void GoToFlash_Click(object sender, RoutedEventArgs e)
    {
        NavigateTo("flash");
    }

    private void GoToRoms_Click(object sender, RoutedEventArgs e) => NavigateTo("flash");

    private void NavigateTo(string tag)
    {
        Navigation.SelectedItem = Navigation.MenuItems
            .OfType<NavigationViewItem>()
            .FirstOrDefault(item => string.Equals(item.Tag?.ToString(), tag, StringComparison.Ordinal));
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
        SelectRomPath(file.Path);
    }

    private async void ChooseDownloadFolderButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker { SuggestedStartLocation = PickerLocationId.Downloads };
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is not null)
        {
            DownloadDirectoryText.Text = folder.Path;
            TrySelectDownloadedRom();
        }
    }

    private void FindDownloadedRomButton_Click(object sender, RoutedEventArgs e)
    {
        if (TrySelectDownloadedRom() && _romPath is { } romPath)
        {
            ShowStatus(L("status.rom_found"), Path.GetFileName(romPath), InfoBarSeverity.Success);
        }
    }

    private async void ListAllowedRomsButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.ListAllowedRomsAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            AllowedRomsText.Text = string.Join(Environment.NewLine, new[] { result.StandardOutput.Trim(), result.StandardError.Trim() }.Where(text => !string.IsNullOrWhiteSpace(text)));
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
        }, L("status.fetching_allowed"));
    }

    private async void DownloadLatestButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.DownloadLatestAsync(device.Index, DownloadDirectoryText.Text, StopAdbToggle.IsOn, cancellationToken);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            AllowedRomsText.Text = result.StandardOutput.Trim();
            var romPath = ExtractDownloadedRomPath(result.StandardOutput) ?? FindDownloadedRom();
            if (romPath is not null)
            {
                SelectRomPath(romPath);
                NavigateTo("flash");
                ShowStatus(L("status.download_ready"), Path.GetFileName(romPath), InfoBarSeverity.Success);
            }
            else
            {
                ShowStatus(L("status.download_complete"), result.StandardOutput.Trim(), InfoBarSeverity.Success);
            }
        }, L("status.downloading_latest"));
    }

    private void SelectRomPath(string path)
    {
        _romPath = path;
        RomPathText.Text = path;
        FlashProgress.Value = 0;
        ProgressPercentText.Text = string.Empty;
        OperationStatusText.Text = L("status.ready_to_flash");
    }

    private bool TrySelectDownloadedRom()
    {
        var romPath = FindDownloadedRom();
        if (romPath is null) return false;
        SelectRomPath(romPath);
        return true;
    }

    private string? FindDownloadedRom()
    {
        var folder = DownloadDirectoryText.Text;
        if (string.IsNullOrWhiteSpace(folder) || !Directory.Exists(folder)) return null;

        var candidates = Directory.EnumerateFiles(folder, "*.zip", SearchOption.TopDirectoryOnly)
            .Select(path => new FileInfo(path))
            .Where(file => file.Length > 0)
            .OrderByDescending(file => file.LastWriteTimeUtc)
            .ToList();
        if (candidates.Count == 0) return null;

        var codename = _backend.Codename ?? _detectedCodename;
        if (!string.IsNullOrWhiteSpace(codename))
        {
            var matching = candidates.FirstOrDefault(file => file.Name.Contains(codename, StringComparison.OrdinalIgnoreCase));
            return matching?.FullName ?? (candidates.Count == 1 ? candidates[0].FullName : null);
        }

        return candidates.Count == 1 ? candidates[0].FullName : null;
    }

    private static string? ExtractDownloadedRomPath(string output)
    {
        const string prefix = "Downloaded to ";
        const string suffix = " (md5 ok)";
        var line = output.Split(['\r', '\n'], StringSplitOptions.RemoveEmptyEntries)
            .FirstOrDefault(value => value.StartsWith(prefix, StringComparison.Ordinal));
        if (line is null) return null;

        var path = line[prefix.Length..];
        if (path.EndsWith(suffix, StringComparison.Ordinal)) path = path[..^suffix.Length];
        return File.Exists(path) ? path : null;
    }

    private static string? DeriveCodename(string device)
    {
        var separator = device.IndexOf('_');
        var codename = separator > 0 ? device[..separator] : device;
        return !string.IsNullOrWhiteSpace(codename)
            && codename.All(character => char.IsLetterOrDigit(character) || character is '-' or '_')
            ? codename
            : null;
    }

    private async void FlashLatestButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }
        if (!await ShowChoiceAsync(L("dialog.flash_latest"), L("dialog.flash_latest_detail"), L("action.download_flash_latest"), L("action.cancel"))) return;
        _operationCancellation = new CancellationTokenSource();
        SetBusy(true, L("status.downloading_latest"));
        try
        {
            var result = await _backend.FlashLatestAsync(device.Index, DownloadDirectoryText.Text, StopAdbToggle.IsOn, HandleBackendEventAsync, _operationCancellation.Token);
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            FlashProgress.Value = 100;
            ProgressPercentText.Text = "100%";
            OperationStatusText.Text = L("status.flash_completed");
            ShowStatus(L("status.flash_completed"), L("status.transfer_ok"), InfoBarSeverity.Success);
        }
        catch (OperationCanceledException)
        {
            OperationStatusText.Text = L("status.cancelled");
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

    private async void DetectButton_Click(object sender, RoutedEventArgs e)
    {
        if (DevicePicker.SelectedItem is not UsbDevice device)
        {
            ShowStatus(L("dialog.no_recovery"), L("dialog.no_recovery_detail"), InfoBarSeverity.Warning);
            return;
        }

        await RunBusyAsync(async cancellationToken =>
        {
            var result = await _backend.DetectAsync(device.Index, StopAdbToggle.IsOn, cancellationToken);
            DiagnosticsText.Text = string.Join(
                Environment.NewLine,
                new[] { result.StandardOutput.Trim(), result.StandardError.Trim() }
                    .Where(value => !string.IsNullOrWhiteSpace(value)));
            if (!result.Succeeded) throw new InvalidOperationException(result.ErrorMessage);
            ShowStatus(L("status.detected"), L("status.handshake_ok"), InfoBarSeverity.Success);
        }, L("status.detecting"));
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
        DownloadLatestButton.IsEnabled = !busy;
        FlashLatestButton.IsEnabled = !busy;
        CancelFlashButton.IsEnabled = busy && _operationCancellation is not null;
        DevicePicker.IsEnabled = !busy;
        RegionProfilePicker.IsEnabled = !busy;
        CodenameText.IsEnabled = !busy;
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
