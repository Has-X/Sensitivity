using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Sensitivity.WinUI.Models;

namespace Sensitivity.WinUI.Services;

public sealed class SensitivityBackend
{
    public string ExecutablePath { get; }
    public string? Profile { get; set; }
    public string? Codename { get; set; }

    public SensitivityBackend()
    {
        ExecutablePath = Path.Combine(AppContext.BaseDirectory, "sensitivity-cli.exe");
    }

    public bool IsAvailable => File.Exists(ExecutablePath);

    public Task<BackendResult> CatalogAsync(string device, string? region, CancellationToken cancellationToken)
    {
        var arguments = new List<string> { "rom-catalog", "--device", device };
        if (!string.IsNullOrWhiteSpace(region)) arguments.AddRange(["--region", region]);
        return RunAsync(arguments, cancellationToken);
    }

    public async Task<IReadOnlyList<UsbDevice>> GetDevicesAsync(CancellationToken cancellationToken)
    {
        var result = await RunAsync(["devices", "--json"], cancellationToken);
        EnsureSuccess(result);
        return JsonSerializer.Deserialize(result.StandardOutput, SensitivityJsonContext.Default.ListUsbDevice) ?? [];
    }

    public async Task<DeviceInfo> GetDeviceInfoAsync(
        int deviceIndex,
        bool stopAdb,
        CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("info");
        arguments.Add("--json");
        var result = await RunAsync(arguments, cancellationToken);
        EnsureSuccess(result);
        return JsonSerializer.Deserialize(result.StandardOutput, SensitivityJsonContext.Default.DeviceInfo)
            ?? throw new InvalidOperationException(LocalizationService.Get("error.backend_incomplete"));
    }

    public async Task<BackendResult> RunDoctorAsync(
        int deviceIndex,
        bool stopAdb,
        CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("doctor");
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> DetectAsync(int deviceIndex, bool stopAdb, CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("detect");
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> ListAllowedRomsAsync(int deviceIndex, bool stopAdb, CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("list-allowed-roms");
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> DownloadLatestAsync(int deviceIndex, string outputDirectory, bool stopAdb, CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.AddRange(["--machine", "download-latest", "--output-dir", outputDirectory]);
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> RebootAsync(
        int deviceIndex,
        bool stopAdb,
        CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("reboot");
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> FormatDataAsync(
        int deviceIndex,
        bool stopAdb,
        CancellationToken cancellationToken)
    {
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.Add("format-data");
        arguments.Add("--yes");
        return await RunAsync(arguments, cancellationToken);
    }

    public async Task<BackendResult> FlashAsync(
        int deviceIndex,
        string romPath,
        bool stopAdb,
        Func<BackendEvent, Task<bool?>> onEvent,
        CancellationToken cancellationToken)
    {
        var controlRoot = Path.Combine(Path.GetTempPath(), "Sensitivity", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(controlRoot);
        var cancelFile = Path.Combine(controlRoot, "cancel");
        var approvalFile = Path.Combine(controlRoot, "approve-wipe");

        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.AddRange([
            "--machine",
            "--cancel-file", cancelFile,
            "--approval-file", approvalFile,
            "flash", romPath
        ]);

        try
        {
            return await RunAsync(
                arguments,
                cancellationToken,
                async backendEvent =>
                {
                    var approved = await onEvent(backendEvent);
                    if (backendEvent.Event == "confirmation_required")
                    {
                        await File.WriteAllTextAsync(
                            approved == true ? approvalFile : cancelFile,
                            string.Empty,
                            CancellationToken.None);
                    }
                },
                cancelFile);
        }
        finally
        {
            try { Directory.Delete(controlRoot, true); } catch { }
        }
    }

    public async Task<BackendResult> FlashLatestAsync(
        int deviceIndex,
        string outputDirectory,
        bool stopAdb,
        Func<BackendEvent, Task<bool?>> onEvent,
        CancellationToken cancellationToken)
    {
        var controlRoot = Path.Combine(Path.GetTempPath(), "Sensitivity", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(controlRoot);
        var cancelFile = Path.Combine(controlRoot, "cancel");
        var approvalFile = Path.Combine(controlRoot, "approve-wipe");
        var arguments = GlobalArguments(deviceIndex, stopAdb);
        arguments.AddRange([
            "--machine", "--cancel-file", cancelFile, "--approval-file", approvalFile,
            "flash-from-latest", "--output-dir", outputDirectory
        ]);
        try
        {
            return await RunAsync(arguments, cancellationToken, async backendEvent =>
            {
                var approved = await onEvent(backendEvent);
                if (backendEvent.Event == "confirmation_required")
                {
                    await File.WriteAllTextAsync(approved == true ? approvalFile : cancelFile, string.Empty, CancellationToken.None);
                }
            }, cancelFile);
        }
        finally
        {
            try { Directory.Delete(controlRoot, true); } catch { }
        }
    }

    public async Task<BackendResult> RunAsync(
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken,
        Func<BackendEvent, Task>? onEvent = null,
        string? gracefulCancelFile = null)
    {
        if (!IsAvailable)
        {
            throw new FileNotFoundException(
                LocalizationService.Get("error.backend_missing_detail"),
                ExecutablePath);
        }

        var operation = Guid.NewGuid().ToString("N");
        var command = arguments.FirstOrDefault(argument => new[] { "rom-catalog", "devices", "info", "doctor", "detect", "list-allowed-roms", "download-latest", "flash-from-latest", "flash", "reboot", "format-data" }.Contains(argument)) ?? "unknown";
        var stopwatch = Stopwatch.StartNew();
        SessionLog.Record("backend.started", new { operation, command, stop_adb = arguments.Contains("stop") });

        using var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = ExecutablePath,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            },
            EnableRaisingEvents = true
        };
        foreach (var argument in arguments)
        {
            process.StartInfo.ArgumentList.Add(argument);
        }
        process.StartInfo.Environment["SENSITIVITY_LANG"] = LocalizationService.CurrentLanguage;
        if (SessionLog.IsAvailable)
            process.StartInfo.Environment["SENSITIVITY_LOG_FILE"] = Path.Combine(SessionLog.SessionDirectory, $"backend-{operation}.jsonl");

        if (!process.Start())
        {
            throw new InvalidOperationException(LocalizationService.Get("error.backend_start"));
        }

        var output = new StringBuilder();
        var errors = new StringBuilder();
        var stdoutTask = ReadOutputAsync(process.StandardOutput, output, onEvent, operation);
        var stderrTask = ReadOutputAsync(process.StandardError, errors, null, operation);
        var exitTask = process.WaitForExitAsync(CancellationToken.None);

        using var cancellationRegistration = cancellationToken.Register(() =>
        {
            SessionLog.Record("backend.cancel_requested", new { operation });
            if (gracefulCancelFile is not null)
            {
                try { File.WriteAllText(gracefulCancelFile, string.Empty); } catch { }
            }
        });

        if (cancellationToken.CanBeCanceled)
        {
            var cancellationTask = Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken)
                .ContinueWith(_ => { }, CancellationToken.None);
            if (await Task.WhenAny(exitTask, cancellationTask) == cancellationTask)
            {
                if (await Task.WhenAny(exitTask, Task.Delay(TimeSpan.FromSeconds(8))) != exitTask)
                {
                    try { process.Kill(true); } catch { }
                }
            }
        }

        // A UI callback can fail while the CLI is paused for approval. Request a
        // graceful close in that case instead of leaving a hidden child process
        // waiting forever for a control file that will never be created.
        if (await Task.WhenAny(exitTask, stdoutTask) == stdoutTask && stdoutTask.IsFaulted)
        {
            if (gracefulCancelFile is not null)
            {
                try { File.WriteAllText(gracefulCancelFile, string.Empty); } catch { }
            }
            if (await Task.WhenAny(exitTask, Task.Delay(TimeSpan.FromSeconds(8))) != exitTask)
            {
                try { process.Kill(true); } catch { }
            }
        }

        await exitTask;
        await Task.WhenAll(stdoutTask, stderrTask);
        SessionLog.Record("backend.exited", new { operation, exit_code = process.ExitCode, duration_ms = stopwatch.ElapsedMilliseconds,
            category = process.ExitCode == 0 ? "success" : SessionLog.ErrorCategory(errors.ToString()) });
        return new BackendResult(process.ExitCode, output.ToString(), errors.ToString());
    }

    public static bool IsUsbOwnershipError(Exception error)
    {
        var message = error.ToString();
        return message.Contains("Claiming interface", StringComparison.OrdinalIgnoreCase)
            || message.Contains("Opening USB device", StringComparison.OrdinalIgnoreCase)
            || message.Contains("Access denied", StringComparison.OrdinalIgnoreCase)
            || message.Contains("busy", StringComparison.OrdinalIgnoreCase);
    }

    private List<string> GlobalArguments(int deviceIndex, bool stopAdb)
    {
        var arguments = new List<string> {
            "--device-index", deviceIndex.ToString(System.Globalization.CultureInfo.InvariantCulture),
            "--adb-policy", stopAdb ? "stop" : "keep"
        };
        if (!string.IsNullOrWhiteSpace(Profile)) arguments.AddRange(["--profile", Profile]);
        if (!string.IsNullOrWhiteSpace(Codename)) arguments.AddRange(["--codename", Codename]);
        return arguments;
    }

    private static async Task ReadOutputAsync(
        StreamReader reader,
        StringBuilder destination,
        Func<BackendEvent, Task>? onEvent,
        string operation)
    {
        while (await reader.ReadLineAsync() is { } line)
        {
            destination.AppendLine(line);
            SessionLog.BackendLine(operation, line);
            if (onEvent is null || !line.StartsWith('{'))
            {
                continue;
            }
            try
            {
                var backendEvent = JsonSerializer.Deserialize(line, SensitivityJsonContext.Default.BackendEvent);
                if (backendEvent is not null)
                {
                    await onEvent(backendEvent);
                }
            }
            catch (JsonException)
            {
                // Human-readable backend lines can coexist with machine events.
            }
        }
    }

    private static void EnsureSuccess(BackendResult result)
    {
        if (!result.Succeeded)
        {
            throw new InvalidOperationException(result.ErrorMessage);
        }
    }
}
