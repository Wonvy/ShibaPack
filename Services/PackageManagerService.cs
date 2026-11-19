using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 安装包管理服务
/// </summary>
public class PackageManagerService
{
    private readonly PackageParserService _parser = new();
    private readonly IconCacheService _iconCache;
    
    public PackageManagerService()
    {
        // 使用安装包图标缓存目录
        _iconCache = new IconCacheService(DataDirectoryService.PackageIconCacheDirectory);
    }
    
    public ObservableCollection<SoftwareGroup> SoftwareGroups { get; } = [];
    public ObservableCollection<PackageInfo> AllPackages { get; } = [];
    
    public event Action? DataChanged;

    /// <summary>
    /// 扫描目录中的所有安装包
    /// </summary>
    public async Task ScanDirectoryAsync(string rootPath)
    {
        if (string.IsNullOrEmpty(rootPath) || !Directory.Exists(rootPath))
            return;

        AllPackages.Clear();
        SoftwareGroups.Clear();

        await Task.Run(async () =>
        {
            var files = Directory.GetFiles(rootPath, "*.*", SearchOption.AllDirectories);
            
            foreach (var file in files)
            {
                var package = _parser.ParsePackage(file);
                if (package != null)
                {
                    // 在后台线程提取图标，避免阻塞 UI
                    package.Icon = await _iconCache.GetFileIconAsync(file);
                    
                    // 切换到 UI 线程添加到集合
                    await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
                    {
                        AddPackageInternal(package);
                    });
                }
            }
        });

        DataChanged?.Invoke();
        await SaveDataAsync();
    }

    /// <summary>
    /// 添加安装包
    /// </summary>
    /// <param name="filePath">文件路径</param>
    /// <param name="rootPath">根目录路径</param>
    /// <param name="platformFolder">平台文件夹名称（win/mac/linux），如果为空则放在根目录</param>
    public async Task<PackageInfo?> AddPackageAsync(string filePath, string rootPath, string? platformFolder = null)
    {
        var package = _parser.ParsePackage(filePath);
        if (package == null)
        {
            System.Diagnostics.Debug.WriteLine($"[AddPackage] 解析失败: {filePath}");
            return null;
        }

        System.Diagnostics.Debug.WriteLine($"[AddPackage] 解析成功: {package.FileName}, 软件名: {package.SoftwareName}");

        // 加载图标
        package.Icon = await _iconCache.GetFileIconAsync(filePath);
        System.Diagnostics.Debug.WriteLine($"[AddPackage] 图标加载: {(package.Icon != null ? "成功" : "失败")}");

        // 获取文件大小
        var fileInfo = new FileInfo(filePath);
        package.FileSize = fileInfo.Length;

        // 如果文件不在根目录下，需要复制或移动
        if (!filePath.StartsWith(rootPath, StringComparison.OrdinalIgnoreCase))
        {
            // 确定目标目录
            string targetDir = rootPath;
            if (!string.IsNullOrEmpty(platformFolder))
            {
                targetDir = Path.Combine(rootPath, platformFolder);
                if (!Directory.Exists(targetDir))
                {
                    Directory.CreateDirectory(targetDir);
                }
            }
            
            var targetPath = Path.Combine(targetDir, package.FileName);
            
            // 检查是否已存在相同文件名和文件大小的文件
            if (File.Exists(targetPath))
            {
                var existingFileInfo = new FileInfo(targetPath);
                if (existingFileInfo.Length == package.FileSize)
                {
                    // 文件名和大小都相同，认为是重复文件
                    System.Diagnostics.Debug.WriteLine($"[AddPackage] 文件已存在: {package.FileName}, 大小: {package.FileSize}");
                    throw new InvalidOperationException($"文件已存在: {package.FileName} (大小: {package.FileSizeText})");
                }
                else
                {
                    // 文件名相同但大小不同，添加时间戳
                    var name = Path.GetFileNameWithoutExtension(package.FileName);
                    var ext = Path.GetExtension(package.FileName);
                    targetPath = Path.Combine(rootPath, $"{name}_{DateTime.Now:yyyyMMddHHmmss}{ext}");
                }
            }
            else
            {
                // 检查根目录下是否有相同文件名和大小的文件（可能在不同位置）
                var existingPackage = AllPackages.FirstOrDefault(p => 
                    p.FileName.Equals(package.FileName, StringComparison.OrdinalIgnoreCase) &&
                    p.FileSize == package.FileSize);
                
                if (existingPackage != null)
                {
                    System.Diagnostics.Debug.WriteLine($"[AddPackage] 文件已存在: {package.FileName}, 大小: {package.FileSize}");
                    throw new InvalidOperationException($"文件已存在: {package.FileName} (大小: {package.FileSizeText})");
                }
            }

            // 根据设置决定复制还是移动
            var settings = AppSettings.Load();
            if (settings.CopyFileOnImport)
            {
                await Task.Run(() => File.Copy(filePath, targetPath, false));
                System.Diagnostics.Debug.WriteLine($"[AddPackage] 文件已复制到: {targetPath}");
            }
            else
            {
                await Task.Run(() => File.Move(filePath, targetPath));
                System.Diagnostics.Debug.WriteLine($"[AddPackage] 文件已移动到: {targetPath}");
            }
            
            package.FullPath = targetPath;
            
            // 重新加载新文件的图标
            package.Icon = await _iconCache.GetFileIconAsync(targetPath);
            System.Diagnostics.Debug.WriteLine($"[AddPackage] 移动后图标重新加载: {(package.Icon != null ? "成功" : "失败")}");
        }
        else
        {
            // 文件已在根目录下，检查是否重复
            var existingPackage = AllPackages.FirstOrDefault(p => 
                p.FileName.Equals(package.FileName, StringComparison.OrdinalIgnoreCase) &&
                p.FileSize == package.FileSize);
            
            if (existingPackage != null)
            {
                System.Diagnostics.Debug.WriteLine($"[AddPackage] 文件已存在: {package.FileName}, 大小: {package.FileSize}");
                throw new InvalidOperationException($"文件已存在: {package.FileName} (大小: {package.FileSizeText})");
            }
            
            package.FullPath = filePath;
        }

        AddPackageInternal(package);
        System.Diagnostics.Debug.WriteLine($"[AddPackage] 已添加到集合, SoftwareGroups 数量: {SoftwareGroups.Count}");
        
        DataChanged?.Invoke();
        await SaveDataAsync();
        
        return package;
    }

    /// <summary>
    /// 整理安装包到对应文件夹
    /// </summary>
    public async Task OrganizePackagesAsync(string rootPath)
    {
        if (string.IsNullOrEmpty(rootPath) || !Directory.Exists(rootPath))
            return;

        await Task.Run(() =>
        {
            // 获取根目录下的所有安装包文件（不在子文件夹中的）
            var files = Directory.GetFiles(rootPath);
            
            foreach (var file in files)
            {
                if (!_parser.IsInstallerPackage(file)) continue;
                
                var package = _parser.ParsePackage(file);
                if (package == null) continue;

                var targetDir = Path.Combine(rootPath, package.SoftwareName);
                if (!Directory.Exists(targetDir))
                {
                    Directory.CreateDirectory(targetDir);
                }

                var targetPath = Path.Combine(targetDir, package.FileName);
                if (targetPath != file)
                {
                    if (File.Exists(targetPath))
                    {
                        var name = Path.GetFileNameWithoutExtension(package.FileName);
                        var ext = Path.GetExtension(package.FileName);
                        targetPath = Path.Combine(targetDir, $"{name}_{DateTime.Now:yyyyMMddHHmmss}{ext}");
                    }
                    File.Move(file, targetPath);
                }
            }
        });

        // 重新扫描
        await ScanDirectoryAsync(rootPath);
    }

    /// <summary>
    /// 删除安装包
    /// </summary>
    public async Task DeletePackageAsync(PackageInfo package)
    {
        if (File.Exists(package.FullPath))
        {
            await Task.Run(() => File.Delete(package.FullPath));
        }

        AllPackages.Remove(package);
        
        var group = SoftwareGroups.FirstOrDefault(g => g.Name == package.SoftwareName);
        if (group != null)
        {
            group.Packages.Remove(package);
            if (group.Packages.Count == 0)
            {
                SoftwareGroups.Remove(group);
            }
        }

        DataChanged?.Invoke();
        await SaveDataAsync();
    }

    /// <summary>
    /// 删除旧版本（保留最新的N个）
    /// </summary>
    public async Task CleanOldVersionsAsync(SoftwareGroup group, int keepCount = 1)
    {
        var toDelete = group.Packages
            .OrderByDescending(p => p.AddedTime)
            .Skip(keepCount)
            .ToList();

        foreach (var package in toDelete)
        {
            await DeletePackageAsync(package);
        }
    }

    /// <summary>
    /// 在文件管理器中打开
    /// </summary>
    public void OpenInExplorer(string path)
    {
        if (File.Exists(path))
        {
            path = Path.GetDirectoryName(path) ?? path;
        }
        
        if (Directory.Exists(path))
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = path,
                UseShellExecute = true
            });
        }
    }

    private void AddPackageInternal(PackageInfo package)
    {
        // 检查是否已存在
        if (AllPackages.Any(p => p.FullPath == package.FullPath))
            return;

        AllPackages.Add(package);

        var group = SoftwareGroups.FirstOrDefault(g => 
            g.Name.Equals(package.SoftwareName, StringComparison.OrdinalIgnoreCase));
        
        if (group == null)
        {
            group = new SoftwareGroup { Name = package.SoftwareName };
            SoftwareGroups.Add(group);
        }
        
        group.Packages.Add(package);
    }

    private async Task SaveDataAsync()
    {
        try
        {
            var filePath = DataDirectoryService.PackagesDataFile;
            var dir = Path.GetDirectoryName(filePath);
            if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
            }

            var data = AllPackages.ToList();
            var json = JsonSerializer.Serialize(data, new JsonSerializerOptions { WriteIndented = true });
            await File.WriteAllTextAsync(filePath, json);
        }
        catch
        {
            // 静默处理保存失败
        }
    }

    public async Task LoadDataAsync()
    {
        try
        {
            // 先清空现有数据
            AllPackages.Clear();
            SoftwareGroups.Clear();
            
            var filePath = DataDirectoryService.PackagesDataFile;
            if (File.Exists(filePath))
            {
                var json = await File.ReadAllTextAsync(filePath);
                var packages = JsonSerializer.Deserialize<List<PackageInfo>>(json);
                
                if (packages != null)
                {
                    foreach (var package in packages)
                    {
                        // 只添加仍然存在的文件
                        if (File.Exists(package.FullPath))
                        {
                            // 加载图标（Bitmap 不会被 JSON 序列化）
                            package.Icon = await _iconCache.GetFileIconAsync(package.FullPath);
                            AddPackageInternal(package);
                        }
                    }
                }
            }
        }
        catch
        {
            // 静默处理加载失败
        }
    }
    
    /// <summary>
    /// 应用软件分类映射
    /// </summary>
    public void ApplyCategoryMappings(Dictionary<string, string> mappings)
    {
        foreach (var group in SoftwareGroups)
        {
            if (mappings.TryGetValue(group.Name, out var categoryId))
            {
                group.CategoryId = categoryId;
            }
        }
    }
}
