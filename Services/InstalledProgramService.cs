using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Microsoft.Win32;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 已安装程序服务
/// </summary>
public class InstalledProgramService
{
    private readonly IconCacheService _iconCache;
    private readonly FileVersionInfoService _versionInfoService;

    public InstalledProgramService(IconCacheService iconCache, FileVersionInfoService versionInfoService)
    {
        _iconCache = iconCache;
        _versionInfoService = versionInfoService;
    }

    /// <summary>
    /// 获取系统已安装的程序列表
    /// </summary>
    public async Task<List<InstalledProgram>> GetInstalledProgramsAsync()
    {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            return new List<InstalledProgram>();
        }

#pragma warning disable CA1416 // 验证平台兼容性
        return await GetInstalledProgramsWindowsAsync();
#pragma warning restore CA1416
    }

    /// <summary>
    /// 从 Windows 注册表获取已安装程序
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private async Task<List<InstalledProgram>> GetInstalledProgramsWindowsAsync()
    {
        var programs = new List<InstalledProgram>();
        var seenDisplayNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

        // 读取注册表的多个位置
        var registryPaths = new[]
        {
            // 64位程序
            @"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            // 32位程序（在64位系统上）
            @"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"
        };

        await Task.Run(async () =>
        {
            foreach (var path in registryPaths)
            {
                try
                {
                    using var key = Registry.LocalMachine.OpenSubKey(path);
                    if (key == null) continue;

                    foreach (var subKeyName in key.GetSubKeyNames())
                    {
                        try
                        {
                            using var subKey = key.OpenSubKey(subKeyName);
                            if (subKey == null) continue;

                            var program = await ParseRegistryEntryAsync(subKey);
                            if (program != null && !seenDisplayNames.Contains(program.DisplayName))
                            {
                                programs.Add(program);
                                seenDisplayNames.Add(program.DisplayName);
                            }
                        }
                        catch
                        {
                            // 跳过无法读取的项
                        }
                    }
                }
                catch
                {
                    // 跳过无法访问的注册表路径
                }
            }

            // 也读取当前用户安装的程序
            try
            {
                using var userKey = Registry.CurrentUser.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall");
                if (userKey != null)
                {
                    foreach (var subKeyName in userKey.GetSubKeyNames())
                    {
                        try
                        {
                            using var subKey = userKey.OpenSubKey(subKeyName);
                            if (subKey == null) continue;

                            var program = await ParseRegistryEntryAsync(subKey);
                            if (program != null && !seenDisplayNames.Contains(program.DisplayName))
                            {
                                programs.Add(program);
                                seenDisplayNames.Add(program.DisplayName);
                            }
                        }
                        catch
                        {
                            // 跳过无法读取的项
                        }
                    }
                }
            }
            catch
            {
                // 跳过
            }
        });

        return programs;
    }

    /// <summary>
    /// 解析注册表项
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private async Task<InstalledProgram?> ParseRegistryEntryAsync(RegistryKey key)
    {
        try
        {
            var displayName = key.GetValue("DisplayName")?.ToString();
            if (string.IsNullOrWhiteSpace(displayName))
                return null;

            // 过滤掉系统组件和更新包
            if (displayName.Contains("Update for", StringComparison.OrdinalIgnoreCase) ||
                displayName.Contains("Security Update", StringComparison.OrdinalIgnoreCase) ||
                displayName.Contains("Hotfix", StringComparison.OrdinalIgnoreCase) ||
                displayName.StartsWith("KB", StringComparison.OrdinalIgnoreCase))
            {
                return null;
            }

            var systemComponent = key.GetValue("SystemComponent");
            if (systemComponent is int intValue && intValue == 1)
                return null;

            var program = new InstalledProgram
            {
                DisplayName = displayName,
                DisplayVersion = key.GetValue("DisplayVersion")?.ToString() ?? "",
                Publisher = key.GetValue("Publisher")?.ToString() ?? "",
                InstallDate = ParseInstallDate(key.GetValue("InstallDate")?.ToString()),
                InstallLocation = key.GetValue("InstallLocation")?.ToString() ?? "",
                UninstallString = key.GetValue("UninstallString")?.ToString() ?? "",
                DisplayIcon = key.GetValue("DisplayIcon")?.ToString() ?? "",
                EstimatedSize = ParseSize(key.GetValue("EstimatedSize")),
                Comments = key.GetValue("Comments")?.ToString() ?? "",
                HelpLink = key.GetValue("HelpLink")?.ToString() ?? "",
                URLInfoAbout = key.GetValue("URLInfoAbout")?.ToString() ?? ""
            };

            // 提取图标
            if (!string.IsNullOrEmpty(program.DisplayIcon))
            {
                var iconPath = ExtractIconPath(program.DisplayIcon);
                if (File.Exists(iconPath))
                {
                    program.Icon = await _iconCache.GetFileIconAsync(iconPath);
                    
                    // 同时提取文件版本信息
                    var versionInfo = _versionInfoService.GetFileVersionInfo(iconPath);
                    if (versionInfo != null)
                    {
                        program.FileDescription = versionInfo.FileDescription;
                        program.ProductVersion = versionInfo.ProductVersion;
                        program.FileVersion = versionInfo.FileVersion;
                    }
                }
            }
            // 如果没有图标路径，尝试从安装位置查找主程序
            else if (!string.IsNullOrEmpty(program.InstallLocation) && Directory.Exists(program.InstallLocation))
            {
                var exeFiles = Directory.GetFiles(program.InstallLocation, "*.exe", SearchOption.TopDirectoryOnly);
                if (exeFiles.Length > 0)
                {
                    var mainExe = exeFiles[0];
                    program.Icon = await _iconCache.GetFileIconAsync(mainExe);
                    
                    var versionInfo = _versionInfoService.GetFileVersionInfo(mainExe);
                    if (versionInfo != null)
                    {
                        program.FileDescription = versionInfo.FileDescription;
                        program.ProductVersion = versionInfo.ProductVersion;
                        program.FileVersion = versionInfo.FileVersion;
                    }
                }
            }

            return program;
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// 从图标字符串中提取文件路径
    /// </summary>
    private static string ExtractIconPath(string iconString)
    {
        if (string.IsNullOrEmpty(iconString))
            return string.Empty;

        // 格式可能是: "C:\Program Files\App\app.exe"
        // 或者: "C:\Program Files\App\app.exe,0"
        // 或者: "C:\Program Files\App\app.ico"
        
        var path = iconString;
        
        // 移除引号
        path = path.Trim('"');
        
        // 如果包含逗号（图标索引），取逗号前的部分
        var commaIndex = path.IndexOf(',');
        if (commaIndex > 0)
        {
            path = path[..commaIndex];
        }

        return path.Trim();
    }

    /// <summary>
    /// 解析安装日期
    /// </summary>
    private static DateTime? ParseInstallDate(string? dateString)
    {
        if (string.IsNullOrEmpty(dateString))
            return null;

        try
        {
            // 格式通常是 YYYYMMDD
            if (dateString.Length == 8 && 
                int.TryParse(dateString[..4], out var year) &&
                int.TryParse(dateString.Substring(4, 2), out var month) &&
                int.TryParse(dateString.Substring(6, 2), out var day))
            {
                return new DateTime(year, month, day);
            }
        }
        catch
        {
            // 解析失败
        }

        return null;
    }

    /// <summary>
    /// 解析大小（单位：KB）
    /// </summary>
    private static long ParseSize(object? sizeValue)
    {
        if (sizeValue is int intSize)
        {
            return intSize * 1024L; // 转换为字节
        }
        return 0;
    }
}
