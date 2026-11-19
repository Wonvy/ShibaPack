using System;
using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 安装包解析服务
/// </summary>
public partial class PackageParserService
{
    private readonly FileVersionInfoService _versionInfoService = new();
    
    // 支持的安装包扩展名
    private static readonly Dictionary<string, PlatformType> ExtensionPlatformMap = new(StringComparer.OrdinalIgnoreCase)
    {
        // Windows 平台
        { ".exe", PlatformType.Windows },
        { ".msi", PlatformType.Windows },
        { ".msix", PlatformType.Windows },
        // macOS 平台
        { ".dmg", PlatformType.MacOS },
        { ".pkg", PlatformType.MacOS },
        { ".app", PlatformType.MacOS },
        // Linux 平台
        { ".deb", PlatformType.Linux },
        { ".rpm", PlatformType.Linux },
        { ".AppImage", PlatformType.Linux },
        { ".snap", PlatformType.Linux },
        // 跨平台/通用格式
        { ".iso", PlatformType.CrossPlatform },
        { ".zip", PlatformType.CrossPlatform },
        { ".7z", PlatformType.CrossPlatform },
        { ".rar", PlatformType.CrossPlatform },
        { ".tar", PlatformType.CrossPlatform },
        { ".gz", PlatformType.CrossPlatform },
        { ".xz", PlatformType.CrossPlatform },
    };

    /// <summary>
    /// 判断文件是否为安装包
    /// </summary>
    public bool IsInstallerPackage(string filePath)
    {
        var ext = Path.GetExtension(filePath);
        return ExtensionPlatformMap.ContainsKey(ext);
    }

    /// <summary>
    /// 解析安装包信息
    /// </summary>
    public PackageInfo? ParsePackage(string filePath)
    {
        if (!File.Exists(filePath)) return null;
        if (!IsInstallerPackage(filePath)) return null;

        var fileInfo = new FileInfo(filePath);
        var fileName = Path.GetFileNameWithoutExtension(filePath);
        var extension = Path.GetExtension(filePath);

        var (softwareName, version) = ParseFileName(fileName);
        
        var package = new PackageInfo
        {
            FileName = fileInfo.Name,
            FullPath = filePath,
            SoftwareName = softwareName,
            Version = version,
            Platform = GetPlatformType(extension, fileName),
            FileSize = fileInfo.Length,
            Extension = extension,
            AddedTime = DateTime.Now,
            ModifiedTime = fileInfo.LastWriteTime
        };
        
        // 对于 Windows 平台的 EXE/MSI 文件，提取版本信息
        if (extension.Equals(".exe", StringComparison.OrdinalIgnoreCase) || 
            extension.Equals(".msi", StringComparison.OrdinalIgnoreCase))
        {
            var versionDetails = _versionInfoService.GetFileVersionInfo(filePath);
            if (versionDetails != null)
            {
                package.ProductName = versionDetails.ProductName;
                package.FileDescription = versionDetails.FileDescription;
                package.CompanyName = versionDetails.CompanyName;
                package.Copyright = versionDetails.Copyright;
                package.Language = versionDetails.Language;
                
                // 如果文件中有版本信息，优先使用
                if (!string.IsNullOrEmpty(versionDetails.ProductVersion))
                {
                    package.Version = versionDetails.ProductVersion;
                }
                else if (!string.IsNullOrEmpty(versionDetails.FileVersion))
                {
                    package.Version = versionDetails.FileVersion;
                }
                
                // 如果有产品名称，优先使用
                if (!string.IsNullOrEmpty(versionDetails.ProductName))
                {
                    package.SoftwareName = versionDetails.ProductName;
                }
            }
        }
        
        return package;
    }

    /// <summary>
    /// 从文件名解析软件名称和版本
    /// </summary>
    private (string Name, string Version) ParseFileName(string fileName)
    {
        // 常见版本号模式
        // 1. Chrome_120.0.6099.130
        // 2. VSCode-1.85.1
        // 3. nodejs-v20.10.0-x64
        // 4. Python3.12.1
        
        // 尝试匹配版本号
        var versionMatch = VersionRegex().Match(fileName);
        
        if (versionMatch.Success)
        {
            var version = versionMatch.Value;
            var nameEndIndex = versionMatch.Index;
            
            // 获取软件名称（版本号之前的部分）
            var name = fileName[..nameEndIndex].TrimEnd('_', '-', '.', ' ', 'v', 'V');
            
            // 清理版本号
            version = version.TrimStart('v', 'V', '_', '-', '.');
            
            return (CleanSoftwareName(name), version);
        }

        return (CleanSoftwareName(fileName), "");
    }

    /// <summary>
    /// 清理软件名称
    /// </summary>
    private static string CleanSoftwareName(string name)
    {
        // 移除常见后缀
        var suffixes = new[] { "_Setup", "_setup", "-Setup", "-setup", "_Installer", "_installer", 
                               "-x64", "-x86", "_x64", "_x86", "-win", "-mac", "-linux",
                               "_win", "_mac", "_linux", "-portable", "_portable" };
        
        foreach (var suffix in suffixes)
        {
            if (name.EndsWith(suffix, StringComparison.OrdinalIgnoreCase))
            {
                name = name[..^suffix.Length];
            }
        }
        
        // 如果名称为空，返回 Unknown
        return string.IsNullOrWhiteSpace(name) ? "Unknown" : name.Trim();
    }

    /// <summary>
    /// 获取平台类型
    /// </summary>
    private static PlatformType GetPlatformType(string extension, string fileName)
    {
        // 先根据扩展名判断
        if (ExtensionPlatformMap.TryGetValue(extension, out var platform))
        {
            return platform;
        }
        
        // 根据文件名关键字判断
        var lowerName = fileName.ToLower();
        if (lowerName.Contains("win") || lowerName.Contains("windows"))
            return PlatformType.Windows;
        if (lowerName.Contains("mac") || lowerName.Contains("darwin") || lowerName.Contains("osx"))
            return PlatformType.MacOS;
        if (lowerName.Contains("linux") || lowerName.Contains("ubuntu") || lowerName.Contains("debian"))
            return PlatformType.Linux;
            
        return PlatformType.Unknown;
    }

    /// <summary>
    /// 获取所有支持的扩展名
    /// </summary>
    public static IEnumerable<string> GetSupportedExtensions() => ExtensionPlatformMap.Keys;

    [GeneratedRegex(@"[vV]?\d+(\.\d+)+([._-]?\w+)?", RegexOptions.Compiled)]
    private static partial Regex VersionRegex();
}
