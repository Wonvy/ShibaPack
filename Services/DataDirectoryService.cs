using System;
using System.IO;

namespace ShibaPack.Services;

/// <summary>
/// 数据目录管理服务
/// </summary>
public static class DataDirectoryService
{
    private static string? _packageRootPath;
    
    /// <summary>
    /// 安装包根目录
    /// </summary>
    public static string PackageRootPath
    {
        get => _packageRootPath ?? string.Empty;
        set
        {
            _packageRootPath = value;
            EnsureDirectoriesExist();
        }
    }
    
    /// <summary>
    /// 数据目录 (.ShibaPackData) - 仅用于存储与安装包相关的数据文件
    /// </summary>
    public static string DataDirectory
    {
        get
        {
            if (string.IsNullOrEmpty(PackageRootPath))
                return GetDefaultDataDirectory();
            
            return Path.Combine(PackageRootPath, ".ShibaPackData");
        }
    }
    
    /// <summary>
    /// 安装包图标缓存目录（保存在安装包目录内）
    /// </summary>
    public static string PackageIconCacheDirectory
    {
        get
        {
            var cachePath = Path.Combine(DataDirectory, "IconCache");
            
            try
            {
                Directory.CreateDirectory(cachePath);
            }
            catch
            {
                // 静默处理
            }
            
            return cachePath;
        }
    }
    
    /// <summary>
    /// 已安装程序图标缓存目录（固定在 AppData，不跟随安装包目录）
    /// </summary>
    public static string InstalledIconCacheDirectory
    {
        get
        {
            var cachePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "ShibaPack", "InstalledIconCache");
            
            try
            {
                Directory.CreateDirectory(cachePath);
            }
            catch
            {
                // 静默处理
            }
            
            return cachePath;
        }
    }
    
    /// <summary>
    /// 图标缓存目录（已废弃，保留用于兼容性）
    /// </summary>
    [Obsolete("使用 PackageIconCacheDirectory 或 InstalledIconCacheDirectory")]
    public static string IconCacheDirectory => InstalledIconCacheDirectory;
    
    /// <summary>
    /// 数据文件目录
    /// </summary>
    public static string DataFilesDirectory => Path.Combine(DataDirectory, "Data");
    
    /// <summary>
    /// 配置文件目录（固定在 AppData，不跟随 PackageRootPath）
    /// </summary>
    public static string ConfigDirectory
    {
        get
        {
            var configPath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                "ShibaPack", "Config");
            
            try
            {
                Directory.CreateDirectory(configPath);
            }
            catch
            {
                // 静默处理
            }
            
            return configPath;
        }
    }
    
    /// <summary>
    /// 安装包数据文件路径
    /// </summary>
    public static string PackagesDataFile => Path.Combine(DataFilesDirectory, "packages.json");
    
    /// <summary>
    /// 分类数据文件路径（保存在安装包目录）
    /// </summary>
    public static string CategoriesDataFile => Path.Combine(DataFilesDirectory, "categories.json");
    
    /// <summary>
    /// 配置文件路径（仅保存安装包目录路径）
    /// </summary>
    public static string SettingsFile => Path.Combine(ConfigDirectory, "settings.json");
    
    /// <summary>
    /// 软件分类映射文件路径（保存在安装包目录）
    /// </summary>
    public static string SoftwareCategoriesFile => Path.Combine(DataFilesDirectory, "software_categories.json");
    
    /// <summary>
    /// 确保所有必要的目录存在
    /// </summary>
    private static void EnsureDirectoriesExist()
    {
        try
        {
            // 已安装程序图标缓存目录始终在 AppData，确保创建
            _ = InstalledIconCacheDirectory; // 访问属性会自动创建目录
            
            // 配置目录始终在 AppData，确保创建
            _ = ConfigDirectory; // 访问属性会自动创建目录
            
            // 如果设置了安装包目录，创建数据目录和安装包图标缓存目录
            if (!string.IsNullOrEmpty(PackageRootPath))
            {
                Directory.CreateDirectory(DataDirectory);
                Directory.CreateDirectory(DataFilesDirectory);
                _ = PackageIconCacheDirectory; // 访问属性会自动创建目录
            }
        }
        catch
        {
            // 静默处理创建失败
        }
    }
    
    /// <summary>
    /// 获取默认数据目录（当未设置安装包目录时使用）
    /// </summary>
    private static string GetDefaultDataDirectory()
    {
        var appDataPath = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "ShibaPack");
        
        try
        {
            Directory.CreateDirectory(appDataPath);
        }
        catch
        {
            // 静默处理
        }
        
        return appDataPath;
    }
    
    /// <summary>
    /// 检查是否已设置有效的安装包目录
    /// </summary>
    public static bool IsPackageRootSet()
    {
        return !string.IsNullOrEmpty(PackageRootPath) && Directory.Exists(PackageRootPath);
    }
}
