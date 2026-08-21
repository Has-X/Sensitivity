using System.Text.Json.Serialization;
using Sensitivity.WinUI.Models;

namespace Sensitivity.WinUI.Services;

[JsonSourceGenerationOptions(PropertyNameCaseInsensitive = true)]
[JsonSerializable(typeof(AppSettings))]
[JsonSerializable(typeof(DeviceInfo))]
[JsonSerializable(typeof(BackendEvent))]
[JsonSerializable(typeof(List<UsbDevice>))]
[JsonSerializable(typeof(Dictionary<string, string>))]
internal sealed partial class SensitivityJsonContext : JsonSerializerContext;
