using Avalonia.Data.Converters;
using Material.Icons;
using System;
using System.Globalization;

namespace ShibaPack.Converters;

/// <summary>
/// 将排序方向转换为图标
/// </summary>
public class SortDirectionToIconConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool ascending)
        {
            return ascending ? MaterialIconKind.ArrowUp : MaterialIconKind.ArrowDown;
        }
        return MaterialIconKind.ArrowUp;
    }

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}

