using System;
using Avalonia;
using Avalonia.Styling;

namespace ShibaPack.Services;

/// <summary>
/// 主题管理服务
/// </summary>
public class ThemeService
{
    private readonly Application? _app;

    public ThemeService()
    {
        _app = Application.Current;
    }

    /// <summary>
    /// 切换主题
    /// </summary>
    public void SetTheme(ThemeMode mode)
    {
        if (_app == null) return;

        _app.RequestedThemeVariant = mode switch
        {
            ThemeMode.Light => ThemeVariant.Light,
            ThemeMode.Dark => ThemeVariant.Dark,
            _ => ThemeVariant.Default
        };
    }

    /// <summary>
    /// 获取当前主题
    /// </summary>
    public ThemeMode GetCurrentTheme()
    {
        if (_app == null) return ThemeMode.Light;

        return _app.RequestedThemeVariant?.Key == ThemeVariant.Dark.Key 
            ? ThemeMode.Dark 
            : ThemeMode.Light;
    }
}

/// <summary>
/// 主题模式
/// </summary>
public enum ThemeMode
{
    Light,
    Dark
}
