using System;
using System.IO;
using System.IO.Compression;
using System.Text.Json;
using System.Threading.Tasks;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 数据导入导出服务
/// </summary>
public class DataImportExportService
{
    /// <summary>
    /// 导出数据包（包含所有JSON配置文件）
    /// </summary>
    /// <param name="exportPath">导出文件路径（.shibapack 格式）</param>
    public async Task<bool> ExportDataAsync(string exportPath)
    {
        try
        {
            var dataDir = DataDirectoryService.DataFilesDirectory;
            var configDir = DataDirectoryService.ConfigDirectory;

            // 创建导出数据对象
            var exportData = new ExportData
            {
                ExportTime = DateTime.Now,
                Version = "1.0",
                PackagesFile = await ReadFileIfExistsAsync(DataDirectoryService.PackagesDataFile),
                CategoriesFile = await ReadFileIfExistsAsync(DataDirectoryService.CategoriesDataFile),
                SettingsFile = await ReadFileIfExistsAsync(DataDirectoryService.SettingsFile)
            };

            // 序列化为JSON
            var json = JsonSerializer.Serialize(exportData, new JsonSerializerOptions 
            { 
                WriteIndented = true,
                Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping
            });

            // 写入文件
            await File.WriteAllTextAsync(exportPath, json);
            
            return true;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 导入数据包
    /// </summary>
    /// <param name="importPath">导入文件路径</param>
    /// <param name="overwrite">是否覆盖现有数据</param>
    public async Task<bool> ImportDataAsync(string importPath, bool overwrite = false)
    {
        try
        {
            if (!File.Exists(importPath))
                return false;

            // 读取导入文件
            var json = await File.ReadAllTextAsync(importPath);
            var importData = JsonSerializer.Deserialize<ExportData>(json);

            if (importData == null)
                return false;

            // 确保目录存在
            EnsureDirectoriesExist();

            // 导入数据文件
            if (!string.IsNullOrEmpty(importData.PackagesFile))
            {
                var packagesPath = DataDirectoryService.PackagesDataFile;
                if (overwrite || !File.Exists(packagesPath))
                {
                    await File.WriteAllTextAsync(packagesPath, importData.PackagesFile);
                }
            }

            if (!string.IsNullOrEmpty(importData.CategoriesFile))
            {
                var categoriesPath = DataDirectoryService.CategoriesDataFile;
                if (overwrite || !File.Exists(categoriesPath))
                {
                    await File.WriteAllTextAsync(categoriesPath, importData.CategoriesFile);
                }
            }

            if (!string.IsNullOrEmpty(importData.SettingsFile))
            {
                var settingsPath = DataDirectoryService.SettingsFile;
                if (overwrite || !File.Exists(settingsPath))
                {
                    await File.WriteAllTextAsync(settingsPath, importData.SettingsFile);
                }
            }

            return true;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 导出完整备份（包含数据和图标缓存）
    /// </summary>
    /// <param name="exportPath">导出ZIP文件路径</param>
    public async Task<bool> ExportFullBackupAsync(string exportPath)
    {
        try
        {
            var tempDir = Path.Combine(Path.GetTempPath(), $"ShibaPack_Backup_{Guid.NewGuid()}");
            Directory.CreateDirectory(tempDir);

            try
            {
                // 复制数据目录
                var dataDir = DataDirectoryService.DataDirectory;
                if (Directory.Exists(dataDir))
                {
                    await CopyDirectoryAsync(dataDir, tempDir);
                }

                // 创建ZIP压缩包
                if (File.Exists(exportPath))
                {
                    File.Delete(exportPath);
                }
                
                ZipFile.CreateFromDirectory(tempDir, exportPath, CompressionLevel.Optimal, false);
                
                return true;
            }
            finally
            {
                // 清理临时目录
                if (Directory.Exists(tempDir))
                {
                    Directory.Delete(tempDir, true);
                }
            }
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 导入完整备份
    /// </summary>
    /// <param name="importPath">ZIP文件路径</param>
    public async Task<bool> ImportFullBackupAsync(string importPath)
    {
        try
        {
            if (!File.Exists(importPath))
                return false;

            var tempDir = Path.Combine(Path.GetTempPath(), $"ShibaPack_Import_{Guid.NewGuid()}");
            
            try
            {
                // 解压ZIP
                ZipFile.ExtractToDirectory(importPath, tempDir);

                // 复制到数据目录
                var dataDir = DataDirectoryService.DataDirectory;
                await CopyDirectoryAsync(tempDir, dataDir);

                return true;
            }
            finally
            {
                // 清理临时目录
                if (Directory.Exists(tempDir))
                {
                    Directory.Delete(tempDir, true);
                }
            }
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// 读取文件内容，如果文件不存在则返回空字符串
    /// </summary>
    private async Task<string> ReadFileIfExistsAsync(string filePath)
    {
        if (File.Exists(filePath))
        {
            return await File.ReadAllTextAsync(filePath);
        }
        return string.Empty;
    }

    /// <summary>
    /// 确保所有必要目录存在
    /// </summary>
    private void EnsureDirectoriesExist()
    {
        var dirs = new[]
        {
            DataDirectoryService.DataFilesDirectory,
            DataDirectoryService.ConfigDirectory,
            DataDirectoryService.PackageIconCacheDirectory
        };

        foreach (var dir in dirs)
        {
            if (!Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
            }
        }
    }

    /// <summary>
    /// 递归复制目录
    /// </summary>
    private async Task CopyDirectoryAsync(string sourceDir, string destDir)
    {
        if (!Directory.Exists(destDir))
        {
            Directory.CreateDirectory(destDir);
        }

        // 复制文件
        foreach (var file in Directory.GetFiles(sourceDir))
        {
            var destFile = Path.Combine(destDir, Path.GetFileName(file));
            File.Copy(file, destFile, true);
        }

        // 递归复制子目录
        foreach (var dir in Directory.GetDirectories(sourceDir))
        {
            var destSubDir = Path.Combine(destDir, Path.GetFileName(dir));
            await CopyDirectoryAsync(dir, destSubDir);
        }

        await Task.CompletedTask;
    }
}

/// <summary>
/// 导出数据结构
/// </summary>
public class ExportData
{
    /// <summary>
    /// 导出时间
    /// </summary>
    public DateTime ExportTime { get; set; }

    /// <summary>
    /// 数据版本
    /// </summary>
    public string Version { get; set; } = "1.0";

    /// <summary>
    /// 安装包数据文件内容（packages.json）
    /// </summary>
    public string PackagesFile { get; set; } = string.Empty;

    /// <summary>
    /// 分类数据文件内容（categories.json）
    /// </summary>
    public string CategoriesFile { get; set; } = string.Empty;

    /// <summary>
    /// 设置文件内容（settings.json）
    /// </summary>
    public string SettingsFile { get; set; } = string.Empty;
}
