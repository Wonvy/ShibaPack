using Avalonia;
using Avalonia.Data.Converters;
using System;
using System.Globalization;

namespace ShibaPack.Converters;

/// <summary>
/// 为"已安装"分类添加底部边框的转换器
/// </summary>
public class InstalledCategoryBorderConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string id && id == "installed")
        {
            return new Thickness(0, 0, 0, 1);
        }
        return new Thickness(0);
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

