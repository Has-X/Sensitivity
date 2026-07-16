using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Sensitivity.WinUI.Models;

namespace Sensitivity.WinUI.Services;

public sealed class SensitivityBackend
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true
    };

    public string ExecutablePath { get; }

    public SensitivityBackend()
    {
        ExecutablePath = Path.Combine(AppContext.BaseDirectory, "sensitivity.exe");
    }

    public bool IsAvailable => File.Exists(ExecutablePath);

    public async Task<IReadOnlyList<UsbDevice>> GetDevicesAsync(CancellationToken cancellationToken)
    {
        var result = await RunAsync(["devices", "--json"], cancellationToken);
        EnsureSuccess(result);
        return JsonSerializer.Deserialize<List<UsbDevice>>(result.StandardOutput, JsonOptions) ?? [];
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
        return JsonSerializer.Deserialize<DeviceInfo>(result.StandardOutput, JsonOptions)
            ?? throw new InvalidOperationException("Sensitivity returned incomplete device information.");
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

    public async Task<BackendResult> RunAsync(
        IReadOnlyList<string> arguments,
        CancellationToken cancellationToken,
        Func<BackendEvent, Task>? onEvent = null,
        string? gracefulCancelFile = null)
    {
        if (!IsAvailable)
        {
            throw new FileNotFoundException(
                "The Sensitivity backend is missing. Reinstall the application or place sensitivity.exe beside Sensitivity.App.exe.",
                ExecutablePath);
        }

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

        if (!process.Start())
        {
            throw new InvalidOperationException("Windows could not start the Sensitivity backend.");
        }

        var output = new StringBuilder();
        var errors = new StringBuilder();
        var stdoutTask = ReadOutputAsync(process.StandardOutput, output, onEvent);
        var stderrTask = ReadOutputAsync(process.StandardError, errors, null);
        var exitTask = process.WaitForExitAsync(CancellationToken.None);

        using var cancellationRegistration = cancellationToken.Register(() =>
        {
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

    private static List<string> GlobalArguments(int deviceIndex, bool stopAdb)
    {
        return [
            "--device-index", deviceIndex.ToString(System.Globalization.CultureInfo.InvariantCulture),
            "--adb-policy", stopAdb ? "stop" : "keep"
        ];
    }

    private static async Task ReadOutputAsync(
        StreamReader reader,
        StringBuilder destination,
        Func<BackendEvent, Task>? onEvent)
    {
        while (await reader.ReadLineAsync() is { } line)
        {
            destination.AppendLine(line);
            if (onEvent is null || !line.StartsWith('{'))
            {
                continue;
            }
            try
            {
                var backendEvent = JsonSerializer.Deserialize<BackendEvent>(line, JsonOptions);
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
