using System;
using System.IO;
using System.Text.Json;
using ShibaPack.Services;

namespace ShibaPack.Models;

/// <summary>
/// 应用配置
/// </summary>
public class AppSettings
{
    /// <summary>
    /// 安装包存储根目录
    /// </summary>
    public string PackageRootPath { get; set; } = string.Empty;
    
    /// <summary>
    /// 是否启用文件监控
    /// </summary>
    public bool EnableFileWatcher { get; set; } = true;
    
    /// <summary>
    /// 是否自动整理文件
    /// </summary>
    public bool AutoOrganize { get; set; } = true;
    
    /// <summary>
    /// 主题模式 (Light/Dark)
    /// </summary>
    public string ThemeMode { get; set; } = "Light";
    
    /// <summary>
    /// 界面语言 (zh-CN/en-US)
    /// </summary>
    public string Language { get; set; } = "zh-CN";
    
    /// <summary>
    /// 拖入文件时是否复制（true=复制，false=移动）
    /// </summary>
    public bool CopyFileOnImport { get; set; } = true;
    
    /// <summary>
    /// 是否启用毛玻璃效果
    /// </summary>
    public bool EnableAcrylic { get; set; } = false;
    
    /// <summary>
    /// 加载配置
    /// </summary>
    public static AppSettings Load()
    {
        try
        {
            var configPath = DataDirectoryService.SettingsFile;
            if (File.Exists(configPath))
            {
                var json = File.ReadAllText(configPath);
                var settings = JsonSerializer.Deserialize<AppSettings>(json) ?? new AppSettings();
                
                // 设置PackageRootPath到DataDirectoryService
                if (!string.IsNullOrEmpty(settings.PackageRootPath))
                {
                    DataDirectoryService.PackageRootPath = settings.PackageRootPath;
                }
                
                return settings;
            }
        }
        catch
        {
            // 配置文件损坏时返回默认配置
        }
        return new AppSettings();
    }
    
    /// <summary>
    /// 保存配置
    /// </summary>
    public void Save()
    {
        try
        {
            // 更新DataDirectoryService的PackageRootPath
            if (!string.IsNullOrEmpty(PackageRootPath))
            {
                DataDirectoryService.PackageRootPath = PackageRootPath;
            }
            
            var configPath = DataDirectoryService.SettingsFile;
            
            var json = JsonSerializer.Serialize(this, new JsonSerializerOptions 
            { 
                WriteIndented = true 
            });
            File.WriteAllText(configPath, json);
        }
        catch
        {
            // 保存失败时静默处理
        }
    }
}
