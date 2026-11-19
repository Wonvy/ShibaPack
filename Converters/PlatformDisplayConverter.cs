using System;
using System.Globalization;
using Avalonia.Data.Converters;

namespace ShibaPack.Converters;

public class PlatformDisplayConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string platform)
        {
            return platform switch
            {
                "win" => "Windows",
                "mac" => "macOS",
                "linux" => "Linux",
                "" or null => "未设置",
                _ => platform
            };
        }
        return "未设置";
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string display)
        {
            return display switch
            {
                "Windows" => "win",
                "macOS" => "mac",
                "Linux" => "linux",
                "未设置" => "",
                _ => display
            };
        }
        return "";
    }
}

