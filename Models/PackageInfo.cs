using System;
using Avalonia.Media.Imaging;

namespace ShibaPack.Models;

/// <summary>
/// 安装包信息
/// </summary>
public class PackageInfo
{
    /// <summary>
    /// 唯一标识
    /// </summary>
    public string Id { get; set; } = Guid.NewGuid().ToString();
    
    /// <summary>
    /// 文件名
    /// </summary>
    public string FileName { get; set; } = string.Empty;
    
    /// <summary>
    /// 完整路径
    /// </summary>
    public string FullPath { get; set; } = string.Empty;
    
    /// <summary>
    /// 软件名称（解析后）
    /// </summary>
    public string SoftwareName { get; set; } = string.Empty;
    
    /// <summary>
    /// 版本号
    /// </summary>
    public string Version { get; set; } = string.Empty;
    
    /// <summary>
    /// 平台类型
    /// </summary>
    public PlatformType Platform { get; set; }

    /// <summary>
    /// 平台显示文本
    /// </summary>
    public string PlatformText => Platform switch
    {
        PlatformType.Windows => "Win",
        PlatformType.MacOS => "Mac",
        PlatformType.Linux => "Linux",
        PlatformType.CrossPlatform => "All",
        _ => ""
    };
    
    /// <summary>
    /// 文件大小（字节）
    /// </summary>
    public long FileSize { get; set; }
    
    /// <summary>
    /// 文件大小显示文本
    /// </summary>
    public string FileSizeText => FormatFileSize(FileSize);
    
    /// <summary>
    /// 添加时间
    /// </summary>
    public DateTime AddedTime { get; set; } = DateTime.Now;
    
    /// <summary>
    /// 文件修改时间
    /// </summary>
    public DateTime ModifiedTime { get; set; } = DateTime.Now;
    
    /// <summary>
    /// 文件修改时间显示文本
    /// </summary>
    public string ModifiedTimeText => ModifiedTime.ToString("yyyy-MM-dd HH:mm");
    
    /// <summary>
    /// 文件扩展名
    /// </summary>
    public string Extension { get; set; } = string.Empty;
    
    /// <summary>
    /// 文件图标
    /// </summary>
    public Bitmap? Icon { get; set; }
    
    /// <summary>
    /// 产品名称（从文件属性获取）
    /// </summary>
    public string? ProductName { get; set; }
    
    /// <summary>
    /// 文件说明（从文件属性获取）
    /// </summary>
    public string? FileDescription { get; set; }
    
    /// <summary>
    /// 公司名称
    /// </summary>
    public string? CompanyName { get; set; }
    
    /// <summary>
    /// 版权信息
    /// </summary>
    public string? Copyright { get; set; }
    
    /// <summary>
    /// 语言
    /// </summary>
    public string? Language { get; set; }
    
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

/// <summary>
/// 平台类型
/// </summary>
public enum PlatformType
{
    Unknown,
    Windows,
    MacOS,
    Linux,
    CrossPlatform
}
