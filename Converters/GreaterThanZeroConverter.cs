using Avalonia.Data.Converters;
using System;
using System.Globalization;

namespace ShibaPack.Converters;

/// <summary>
/// 将数值/计数转换为 bool：>0 为 true，否则 false
/// </summary>
public class GreaterThanZeroConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value == null) return false;

        try
        {
            return value switch
            {
                int i => i > 0,
                long l => l > 0,
                double d => d > 0,
                float f => f > 0,
                _ => false
            };
        }
        catch
        {
            return false;
        }
    }

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotSupportedException();
    }
}


