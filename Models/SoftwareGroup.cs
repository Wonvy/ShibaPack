using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using System.Runtime.CompilerServices;

namespace ShibaPack.Models;

/// <summary>
/// 软件分组（同一软件的多个版本）
/// </summary>
public class SoftwareGroup : INotifyPropertyChanged
{
    private bool _isGroupExpanded;
    
    public event PropertyChangedEventHandler? PropertyChanged;
    
    private void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
    
    /// <summary>
    /// 软件名称
    /// </summary>
    public string Name { get; set; } = string.Empty;
    
    /// <summary>
    /// 所属分类ID
    /// </summary>
    public string CategoryId { get; set; } = "uncategorized";
    
    /// <summary>
    /// 该软件的所有安装包
    /// </summary>
    public ObservableCollection<PackageInfo> Packages { get; set; } = [];
    
    /// <summary>
    /// 安装包数量
    /// </summary>
    public int PackageCount => Packages.Count;

    /// <summary>
    /// 是否包含多个版本（用于 UI 更直观提示）
    /// </summary>
    public bool HasMultipleVersions => PackageCount > 1;

    /// <summary>
    /// 多版本标签文本（例如：3 版本；单版本时为空）
    /// </summary>
    public string VersionBadgeText => HasMultipleVersions ? $"{PackageCount} 版本" : string.Empty;
    
    /// <summary>
    /// 分组展开状态（列表视图：展开显示所有版本）
    /// </summary>
    public bool IsGroupExpanded
    {
        get => _isGroupExpanded;
        set
        {
            if (_isGroupExpanded != value)
            {
                _isGroupExpanded = value;
                OnPropertyChanged();
            }
        }
    }
    
    /// <summary>
    /// 最新版本
    /// </summary>
    public string LatestVersion => Packages.OrderByDescending(p => p.AddedTime).FirstOrDefault()?.Version ?? "";
    
    /// <summary>
    /// 总大小
    /// </summary>
    public long TotalSize => Packages.Sum(p => p.FileSize);
    
    /// <summary>
    /// 总大小显示文本
    /// </summary>
    public string TotalSizeText => FormatFileSize(TotalSize);
    
    /// <summary>
    /// 支持的平台
    /// </summary>
    public string PlatformsText
    {
        get
        {
            var platforms = Packages.Select(p => p.Platform).Distinct().ToList();
            var names = new List<string>();
            if (platforms.Contains(PlatformType.Windows)) names.Add("Win");
            if (platforms.Contains(PlatformType.MacOS)) names.Add("Mac");
            if (platforms.Contains(PlatformType.Linux)) names.Add("Linux");
            return string.Join(" | ", names);
        }
    }
    
    /// <summary>
    /// 软件描述（来自最新版本的文件说明）
    /// </summary>
    public string Description
    {
        get
        {
            var latestPackage = Packages.OrderByDescending(p => p.AddedTime).FirstOrDefault();
            return latestPackage?.FileDescription ?? string.Empty;
        }
    }
    
    /// <summary>
    /// 最新修改时间（来自最新版本）
    /// </summary>
    public string LatestModifiedTimeText
    {
        get
        {
            var latestPackage = Packages.OrderByDescending(p => p.ModifiedTime).FirstOrDefault();
            return latestPackage?.ModifiedTimeText ?? "";
        }
    }
    
    private static string FormatFileSize(long bytes)
    {
        string[] sizes = ["B", "KB", "MB", "GB"];
        int order = 0;
        double size = bytes;
        while (size >= 1024 && order < sizes.Length - 1)
        {
            order++;
            size /= 1024;
        }
        return $"{size:0.##} {sizes[order]}";
    }
}
