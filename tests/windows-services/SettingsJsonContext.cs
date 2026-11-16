using System.Text.Json.Serialization;

namespace Sensitivity.WinUI.Services;

[JsonSerializable(typeof(AppSettings))]
internal sealed partial class SensitivityJsonContext : JsonSerializerContext;
