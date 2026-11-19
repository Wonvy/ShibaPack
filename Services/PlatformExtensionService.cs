using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 平台文件后缀管理服务
/// </summary>
public class PlatformExtensionService
{
    private const string ConfigFileName = "platform_extensions.json";
    private PlatformExtensionsConfig _config = new();
    
    private string ConfigFilePath => Path.Combine(DataDirectoryService.ConfigDirectory, ConfigFileName);
    
    public PlatformExtensionService()
    {
        LoadConfig();
        InitializeDefaults();
    }
    
    /// <summary>
    /// 获取所有平台后缀配置
    /// </summary>
    public List<PlatformFileExtension> GetAllExtensions()
    {
        return _config.Extensions.ToList();
    }
    
    /// <summary>
    /// 根据文件路径或后缀获取平台
    /// </summary>
    public string? GetPlatformByExtension(string filePathOrExtension)
    {
        var extension = GetExtension(filePathOrExtension);
        if (string.IsNullOrEmpty(extension))
            return null;
            
        var ext = extension.ToLowerInvariant();
        var config = _config.Extensions.FirstOrDefault(e => 
            e.Extension.Equals(ext, StringComparison.OrdinalIgnoreCase));
            
        return config?.Platform;
    }
    
    /// <summary>
    /// 添加或更新平台后缀配置
    /// </summary>
    public void AddOrUpdateExtension(string extension, string platform)
    {
        var ext = GetExtension(extension).ToLowerInvariant();
        var existing = _config.Extensions.FirstOrDefault(e => 
            e.Extension.Equals(ext, StringComparison.OrdinalIgnoreCase));
            
        if (existing != null)
        {
            existing.Platform = platform;
        }
        else
        {
            _config.Extensions.Add(new PlatformFileExtension
            {
                Extension = ext,
                Platform = platform
            });
        }
        
        SaveConfig();
    }
    
    /// <summary>
    /// 删除平台后缀配置
    /// </summary>
    public void RemoveExtension(string extension)
    {
        var ext = GetExtension(extension).ToLowerInvariant();
        _config.Extensions.RemoveAll(e => 
            e.Extension.Equals(ext, StringComparison.OrdinalIgnoreCase));
        SaveConfig();
    }
    
    /// <summary>
    /// 检查是否为文件夹
    /// </summary>
    public bool IsDirectory(string path)
    {
        try
        {
            return Directory.Exists(path);
        }
        catch
        {
            return false;
        }
    }
    
    /// <summary>
    /// 从文件路径或后缀字符串中提取扩展名（不含点号）
    /// </summary>
    private string GetExtension(string filePathOrExtension)
    {
        if (string.IsNullOrEmpty(filePathOrExtension))
            return string.Empty;
            
        // 如果包含路径分隔符，说明是文件路径
        if (filePathOrExtension.Contains(Path.DirectorySeparatorChar) || 
            filePathOrExtension.Contains(Path.AltDirectorySeparatorChar))
        {
            var ext = Path.GetExtension(filePathOrExtension);
            return string.IsNullOrEmpty(ext) ? string.Empty : ext.TrimStart('.');
        }
        
        // 否则直接返回（去除点号）
        return filePathOrExtension.TrimStart('.');
    }
    
    /// <summary>
    /// 初始化默认配置
    /// </summary>
    private void InitializeDefaults()
    {
        var defaults = new Dictionary<string, string>
        {
            // Windows
            { "exe", "win" },
            { "msi", "win" },
            { "msix", "win" },
            { "appx", "win" },
            { "appxbundle", "win" },
            { "msixbundle", "win" },
            
            // macOS
            { "dmg", "mac" },
            { "pkg", "mac" },
            { "app", "mac" },
            
            // Linux
            { "deb", "linux" },
            { "rpm", "linux" },
            { "AppImage", "linux" },
            { "snap", "linux" },
            { "flatpak", "linux" },
            { "tar.gz", "linux" },
            { "tar.xz", "linux" },
            
            // 通用压缩包（需要用户选择）
            { "zip", "" },
            { "7z", "" },
            { "rar", "" },
            { "tar", "" }
        };
        
        bool hasNewDefaults = false;
        foreach (var kvp in defaults)
        {
            var existing = _config.Extensions.FirstOrDefault(e => 
                e.Extension.Equals(kvp.Key, StringComparison.OrdinalIgnoreCase));
            if (existing == null)
            {
                _config.Extensions.Add(new PlatformFileExtension
                {
                    Extension = kvp.Key.ToLowerInvariant(),
                    Platform = kvp.Value
                });
                hasNewDefaults = true;
            }
        }
        
        if (hasNewDefaults)
        {
            SaveConfig();
        }
    }
    
    /// <summary>
    /// 加载配置
    /// </summary>
    private void LoadConfig()
    {
        try
        {
            if (File.Exists(ConfigFilePath))
            {
                var json = File.ReadAllText(ConfigFilePath);
                _config = JsonSerializer.Deserialize<PlatformExtensionsConfig>(json) ?? new PlatformExtensionsConfig();
            }
        }
        catch
        {
            _config = new PlatformExtensionsConfig();
        }
    }
    
    /// <summary>
    /// 保存配置
    /// </summary>
    private void SaveConfig()
    {
        try
        {
            var json = JsonSerializer.Serialize(_config, new JsonSerializerOptions 
            { 
                WriteIndented = true 
            });
            File.WriteAllText(ConfigFilePath, json);
        }
        catch
        {
            // 保存失败时静默处理
        }
    }
}

