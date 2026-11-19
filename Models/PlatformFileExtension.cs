using System.Collections.Generic;
using System.ComponentModel;

namespace ShibaPack.Models;

/// <summary>
/// 平台文件后缀配置
/// </summary>
public class PlatformFileExtension : INotifyPropertyChanged
{
    private string _extension = string.Empty;
    private string? _platform = string.Empty;
    
    /// <summary>
    /// 文件后缀（不含点号，例如：exe, dmg, deb）
    /// </summary>
    public string Extension
    {
        get => _extension;
        set
        {
            _extension = value;
            OnPropertyChanged(nameof(Extension));
        }
    }
    
    /// <summary>
    /// 平台标识：win, mac, linux（空字符串或null表示未设置）
    /// </summary>
    public string? Platform
    {
        get => _platform;
        set
        {
            _platform = value;
            OnPropertyChanged(nameof(Platform));
        }
    }
    
    public event PropertyChangedEventHandler? PropertyChanged;
    
    protected virtual void OnPropertyChanged(string propertyName)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}

/// <summary>
/// 平台文件后缀配置集合
/// </summary>
public class PlatformExtensionsConfig
{
    /// <summary>
    /// 平台后缀映射列表
    /// </summary>
    public List<PlatformFileExtension> Extensions { get; set; } = new();
}

