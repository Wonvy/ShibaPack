using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Material.Icons;

namespace ShibaPack.Converters;

/// <summary>
/// 将图标名称转换为 MaterialIconKind
/// </summary>
public class IconNameToMaterialIconKindConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not string iconName || string.IsNullOrEmpty(iconName))
            return MaterialIconKind.Package; // 默认图标

        // 尝试将字符串转换为 MaterialIconKind 枚举
        if (Enum.TryParse<MaterialIconKind>(iconName, true, out var iconKind))
        {
            return iconKind;
        }

        // 如果转换失败，返回默认图标
        return MaterialIconKind.Package;
    }

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is MaterialIconKind iconKind)
        {
            return iconKind.ToString();
        }
        return MaterialIconKind.Package.ToString();
    }
}
