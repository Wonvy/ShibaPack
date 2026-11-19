using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;

namespace ShibaPack.Services;

/// <summary>
/// 文件版本信息提取服务
/// </summary>
public class FileVersionInfoService
{
    /// <summary>
    /// 获取文件版本信息
    /// </summary>
    public FileVersionDetails? GetFileVersionInfo(string filePath)
    {
        if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            return null;

        try
        {
            if (RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
            {
#pragma warning disable CA1416 // 验证平台兼容性
                return GetFileVersionInfoWindows(filePath);
#pragma warning restore CA1416
            }
            
            // 其他平台暂不支持
            return null;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// Windows 平台获取文件版本信息
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private FileVersionDetails? GetFileVersionInfoWindows(string filePath)
    {
        try
        {
            var versionInfo = FileVersionInfo.GetVersionInfo(filePath);
            
            return new FileVersionDetails
            {
                ProductName = versionInfo.ProductName,
                FileDescription = versionInfo.FileDescription,
                CompanyName = versionInfo.CompanyName,
                Copyright = versionInfo.LegalCopyright,
                FileVersion = versionInfo.FileVersion,
                ProductVersion = versionInfo.ProductVersion,
                Language = versionInfo.Language,
                InternalName = versionInfo.InternalName,
                OriginalFilename = versionInfo.OriginalFilename,
                Comments = versionInfo.Comments
            };
        }
        catch
        {
            return null;
        }
    }
}

/// <summary>
/// 文件版本详细信息
/// </summary>
public class FileVersionDetails
{
    /// <summary>
    /// 产品名称
    /// </summary>
    public string? ProductName { get; set; }
    
    /// <summary>
    /// 文件说明
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
    /// 文件版本
    /// </summary>
    public string? FileVersion { get; set; }
    
    /// <summary>
    /// 产品版本
    /// </summary>
    public string? ProductVersion { get; set; }
    
    /// <summary>
    /// 语言
    /// </summary>
    public string? Language { get; set; }
    
    /// <summary>
    /// 内部名称
    /// </summary>
    public string? InternalName { get; set; }
    
    /// <summary>
    /// 原始文件名
    /// </summary>
    public string? OriginalFilename { get; set; }
    
    /// <summary>
    /// 注释
    /// </summary>
    public string? Comments { get; set; }
}
