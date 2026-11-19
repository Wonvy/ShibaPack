using System;
using Avalonia.Media.Imaging;

namespace ShibaPack.Models;

/// <summary>
/// 已安装的程序信息
/// </summary>
public class InstalledProgram
{
    /// <summary>
    /// 显示名称
    /// </summary>
    public string DisplayName { get; set; } = string.Empty;
    
    /// <summary>
    /// 显示版本
    /// </summary>
    public string DisplayVersion { get; set; } = string.Empty;
    
    /// <summary>
    /// 发布者/公司
    /// </summary>
    public string Publisher { get; set; } = string.Empty;
    
    /// <summary>
    /// 安装日期
    /// </summary>
    public DateTime? InstallDate { get; set; }
    
    /// <summary>
    /// 安装位置
    /// </summary>
    public string InstallLocation { get; set; } = string.Empty;
    
    /// <summary>
    /// 卸载命令
    /// </summary>
    public string UninstallString { get; set; } = string.Empty;
    
    /// <summary>
    /// 显示图标路径
    /// </summary>
    public string DisplayIcon { get; set; } = string.Empty;
    
    /// <summary>
    /// 程序图标
    /// </summary>
    public Bitmap? Icon { get; set; }
    
    /// <summary>
    /// 估计大小（字节）
    /// </summary>
    public long EstimatedSize { get; set; }
    
    /// <summary>
    /// 大小文本
    /// </summary>
    public string SizeText => FormatFileSize(EstimatedSize);
    
    /// <summary>
    /// 注释
    /// </summary>
    public string Comments { get; set; } = string.Empty;
    
    /// <summary>
    /// 帮助链接
    /// </summary>
    public string HelpLink { get; set; } = string.Empty;
    
    /// <summary>
    /// 关于链接
    /// </summary>
    public string URLInfoAbout { get; set; } = string.Empty;
    
    /// <summary>
    /// 文件说明
    /// </summary>
    public string? FileDescription { get; set; }
    
    /// <summary>
    /// 产品版本
    /// </summary>
    public string? ProductVersion { get; set; }
    
    /// <summary>
    /// 文件版本
    /// </summary>
    public string? FileVersion { get; set; }
    
    /// <summary>
    /// 安装日期文本
    /// </summary>
    public string InstallDateText => InstallDate?.ToString("yyyy-MM-dd") ?? "未知";
    
    private static string FormatFileSize(long bytes)
    {
        if (bytes == 0) return "未知";
        
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
