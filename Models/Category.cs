using System;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using ShibaPack.Resources;

namespace ShibaPack.Models;

/// <summary>
/// 软件分类
/// </summary>
public class Category : INotifyPropertyChanged
{
    private int _softwareCount;
    private string _name = string.Empty;
    private string _icon = "Package";
    private bool _isExpanded = true;
    
    public event PropertyChangedEventHandler? PropertyChanged;
    
    protected virtual void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
    /// <summary>
    /// 分类ID
    /// </summary>
    public string Id { get; set; } = Guid.NewGuid().ToString();
    
    /// <summary>
    /// 分类名称
    /// </summary>
    public string Name
    {
        get => _name;
        set
        {
            if (_name != value)
            {
                _name = value;
                OnPropertyChanged();
            }
        }
    }
    
    /// <summary>
    /// 分类图标（Lucide 图标名称）
    /// </summary>
    public string Icon
    {
        get => _icon;
        set
        {
            if (_icon != value)
            {
                _icon = value;
                OnPropertyChanged();
            }
        }
    }
    
    /// <summary>
    /// 父分类ID（如果是子分类）
    /// </summary>
    public string? ParentId { get; set; }
    
    /// <summary>
    /// 子分类列表
    /// </summary>
    public ObservableCollection<Category> Children { get; set; } = new();
    
    /// <summary>
    /// 是否展开（用于UI显示）
    /// </summary>
    public bool IsExpanded
    {
        get => _isExpanded;
        set
        {
            if (_isExpanded != value)
            {
                _isExpanded = value;
                OnPropertyChanged();
            }
        }
    }
    
    /// <summary>
    /// 排序顺序
    /// </summary>
    public int SortOrder { get; set; }
    
    /// <summary>
    /// 创建时间
    /// </summary>
    public DateTime CreatedTime { get; set; } = DateTime.Now;
    
    /// <summary>
    /// 该分类下的软件数量
    /// </summary>
    public int SoftwareCount
    {
        get => _softwareCount;
        set
        {
            if (_softwareCount != value)
            {
                _softwareCount = value;
                OnPropertyChanged();
            }
        }
    }
}

/// <summary>
/// 预定义分类
/// </summary>
public static class PredefinedCategories
{
    public static readonly Category InstalledPrograms = new()
    {
        Id = "installed",
        Name = "已安装",
        Icon = "Harddisk",
        SortOrder = -4
    };
    
    public static readonly Category AllSoftware = new()
    {
        Id = "all",
        Name = "所有软件",
        Icon = "Apps",
        SortOrder = -3
    };
    
    public static readonly Category Uncategorized = new()
    {
        Id = "uncategorized",
        Name = "未分类",
        Icon = "Package",
        SortOrder = -1
    };
    
    public static Category[] GetDefaults() => new[]
    {
        new Category { Id = "dev", Name = "开发工具", Icon = "CodeTags", SortOrder = 1 },
        new Category { Id = "design", Name = "设计工具", Icon = "Palette", SortOrder = 2 },
        new Category { Id = "productivity", Name = "效率工具", Icon = "LightningBolt", SortOrder = 3 },
        new Category { Id = "media", Name = "媒体工具", Icon = "Image", SortOrder = 4 },
        new Category { Id = "game", Name = "游戏娱乐", Icon = "GamepadVariant", SortOrder = 5 },
        new Category { Id = "utility", Name = "系统工具", Icon = "Wrench", SortOrder = 6 }
    };
}
