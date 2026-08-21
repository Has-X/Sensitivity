using System.Text.Json.Serialization;
using Sensitivity.WinUI.Services;

namespace Sensitivity.WinUI.Models;

public sealed class UsbDevice
{
    [JsonPropertyName("index")]
    public int Index { get; set; }

    [JsonPropertyName("bus")]
    public int Bus { get; set; }

    [JsonPropertyName("address")]
    public int Address { get; set; }

    [JsonPropertyName("vendor_id")]
    public int VendorId { get; set; }

    [JsonPropertyName("product_id")]
    public int ProductId { get; set; }

    [JsonPropertyName("protocol")]
    public int Protocol { get; set; }

    [JsonIgnore]
    public string DisplayName => $"{LocalizationService.Get("label.recovery_device")} {Index + 1}  ·  {VendorId:x4}:{ProductId:x4}  ·  USB {Bus}/{Address}";
}

public sealed class DeviceInfo
{
    [JsonPropertyName("device")]
    public string Device { get; set; } = "—";

    [JsonPropertyName("sn")]
    public string Serial { get; set; } = "—";

    [JsonPropertyName("version")]
    public string Version { get; set; } = "—";

    [JsonPropertyName("codebase")]
    public string Codebase { get; set; } = "—";

    [JsonPropertyName("branch")]
    public string Branch { get; set; } = "—";

    [JsonPropertyName("language")]
    public string Language { get; set; } = "—";

    [JsonPropertyName("region")]
    public string Region { get; set; } = "—";

    [JsonPropertyName("romzone")]
    public string RomZone { get; set; } = "—";
}

public sealed class BackendEvent
{
    [JsonPropertyName("event")]
    public string Event { get; set; } = string.Empty;

    [JsonPropertyName("kind")]
    public string? Kind { get; set; }

    [JsonPropertyName("message")]
    public string? Message { get; set; }

    [JsonPropertyName("current")]
    public long Current { get; set; }

    [JsonPropertyName("total")]
    public long Total { get; set; }
}

public sealed record BackendResult(int ExitCode, string StandardOutput, string StandardError)
{
    public bool Succeeded => ExitCode == 0;

    public string ErrorMessage => string.IsNullOrWhiteSpace(StandardError)
        ? LocalizationService.Get("error.operation_failed")
        : StandardError.Trim();
}
