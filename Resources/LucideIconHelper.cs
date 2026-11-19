using Avalonia.Media;

namespace ShibaPack.Resources;

/// <summary>
/// Lucide 图标路径几何辅助类
/// </summary>
public static class LucideIconHelper
{
    /// <summary>
    /// 获取图标路径几何
    /// </summary>
    public static Geometry GetGeometry(string iconPath)
    {
        return Geometry.Parse(iconPath);
    }
}
