using System.Linq;
using System.Reflection;

namespace AwayPhotoRawEditor.App;

/// <summary>App 版本（關於視窗顯示用）。
/// 規則：每次交付更新把第三位 +1，並同步 csproj 的 &lt;Version&gt;。</summary>
public static class AppVersion
{
    public const string Version = "v1.0.5";

    /// <summary>編譯時間（csproj 以 AssemblyMetadata "BuildTime" 於建置時寫入）。</summary>
    public static string BuildTime =>
        typeof(AppVersion).Assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .FirstOrDefault(a => a.Key == "BuildTime")?.Value ?? "";
}
