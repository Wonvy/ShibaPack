using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Platform.Storage;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ShibaPack.Models;
using ShibaPack.Services;

namespace ShibaPack.ViewModels;

public partial class SettingsViewModel : ViewModelBase
{
    private readonly AppSettings _settings;
    private readonly ThemeService _themeService = new();
    private readonly LocalizationService _localization = LocalizationService.Instance;
    private readonly DataImportExportService _importExportService = new();
    private readonly PlatformExtensionService _platformExtensionService = new();

    [ObservableProperty]
    private string _packageRootPath = string.Empty;

    [ObservableProperty]
    private bool _autoOrganize = true;

    [ObservableProperty]
    private bool _copyFileOnImport = true;
    
    public bool IsCopyFileOnImport
    {
        get => CopyFileOnImport;
        set
        {
            if (value && !CopyFileOnImport)
            {
                CopyFileOnImport = true;
                OnPropertyChanged();
                OnPropertyChanged(nameof(IsMoveFileOnImport));
            }
        }
    }
    
    public bool IsMoveFileOnImport
    {
        get => !CopyFileOnImport;
        set
        {
            if (value && CopyFileOnImport)
            {
                CopyFileOnImport = false;
                OnPropertyChanged();
                OnPropertyChanged(nameof(IsCopyFileOnImport));
            }
        }
    }

    [ObservableProperty]
    private bool _isDarkMode = false;
    
    [ObservableProperty]
    private bool _enableAcrylic = false;

    [ObservableProperty]
    private string _statusMessage = string.Empty;
    
    [ObservableProperty]
    private LanguageOption? _selectedLanguage;
    
    [ObservableProperty]
    private string _selectedCategory = string.Empty;
    
    public List<LanguageOption> AvailableLanguages { get; } = LocalizationService.GetSupportedLanguages();
    
    [ObservableProperty]
    private string _windowsExtensions = string.Empty;
    
    [ObservableProperty]
    private string _macExtensions = string.Empty;
    
    [ObservableProperty]
    private string _linuxExtensions = string.Empty;
    
    public List<string> SettingsCategories => new() 
    { 
        _localization["CategoryAppearance"], 
        _localization["CategoryStorage"], 
        _localization["CategoryBehavior"], 
        _localization["CategoryCache"], 
        _localization["CategoryData"] 
    };
    
    public List<string> SettingsCategoryKeys => new() { "Appearance", "Storage", "Behavior", "Cache", "Data" };
    
    public LocalizationService Localization => _localization;

    // 用于存储窗口引用
    public Avalonia.Controls.TopLevel? TopLevel { get; set; }

    public SettingsViewModel()
    {
        _settings = AppSettings.Load();
        LoadSettings();
        LoadPlatformExtensions();
        
        // 监听语言变化，更新分类列表
        _localization.PropertyChanged += (s, e) =>
        {
            if (e.PropertyName == "CurrentLanguage" || e.PropertyName?.StartsWith("Item[") == true)
            {
                OnPropertyChanged(nameof(SettingsCategories));
            }
        };
        
    }
    
    partial void OnWindowsExtensionsChanged(string value)
    {
        SavePlatformExtensions();
    }
    
    partial void OnMacExtensionsChanged(string value)
    {
        SavePlatformExtensions();
    }
    
    partial void OnLinuxExtensionsChanged(string value)
    {
        SavePlatformExtensions();
    }
    
    private void LoadPlatformExtensions()
    {
        var extensions = _platformExtensionService.GetAllExtensions();
        
        var winExts = extensions.Where(e => e.Platform == "win").Select(e => e.Extension);
        var macExts = extensions.Where(e => e.Platform == "mac").Select(e => e.Extension);
        var linuxExts = extensions.Where(e => e.Platform == "linux").Select(e => e.Extension);
        
        WindowsExtensions = string.Join(",", winExts);
        MacExtensions = string.Join(",", macExts);
        LinuxExtensions = string.Join(",", linuxExts);
    }
    
    private void SavePlatformExtensions()
    {
        // 清除所有现有配置（只清除有平台的后缀，保留空平台的后缀如zip）
        var allExtensions = _platformExtensionService.GetAllExtensions();
        foreach (var ext in allExtensions)
        {
            if (!string.IsNullOrEmpty(ext.Platform) && (ext.Platform == "win" || ext.Platform == "mac" || ext.Platform == "linux"))
            {
                _platformExtensionService.RemoveExtension(ext.Extension);
            }
        }
        
        // 保存Windows后缀
        if (!string.IsNullOrWhiteSpace(WindowsExtensions))
        {
            var exts = WindowsExtensions.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            foreach (var ext in exts)
            {
                if (!string.IsNullOrWhiteSpace(ext))
                {
                    _platformExtensionService.AddOrUpdateExtension(ext.Trim(), "win");
                }
            }
        }
        
        // 保存macOS后缀
        if (!string.IsNullOrWhiteSpace(MacExtensions))
        {
            var exts = MacExtensions.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            foreach (var ext in exts)
            {
                if (!string.IsNullOrWhiteSpace(ext))
                {
                    _platformExtensionService.AddOrUpdateExtension(ext.Trim(), "mac");
                }
            }
        }
        
        // 保存Linux后缀
        if (!string.IsNullOrWhiteSpace(LinuxExtensions))
        {
            var exts = LinuxExtensions.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            foreach (var ext in exts)
            {
                if (!string.IsNullOrWhiteSpace(ext))
                {
                    _platformExtensionService.AddOrUpdateExtension(ext.Trim(), "linux");
                }
            }
        }
    }

    private void LoadSettings()
    {
        PackageRootPath = _settings.PackageRootPath;
        AutoOrganize = _settings.AutoOrganize;
        CopyFileOnImport = _settings.CopyFileOnImport;
        IsDarkMode = _settings.ThemeMode == "Dark";
        EnableAcrylic = _settings.EnableAcrylic;
        
        // 加载语言设置
        var langCode = _settings.Language;
        _localization.CurrentLanguage = langCode;
        SelectedLanguage = AvailableLanguages.FirstOrDefault(l => l.Code == langCode);
        
        // 初始化选中的分类为本地化的名称
        SelectedCategory = _localization["CategoryAppearance"];
    }

    partial void OnIsDarkModeChanged(bool value)
    {
        _themeService.SetTheme(value ? ThemeMode.Dark : ThemeMode.Light);
        _settings.ThemeMode = value ? "Dark" : "Light";
        SaveSettings();
    }

    partial void OnAutoOrganizeChanged(bool value)
    {
        _settings.AutoOrganize = value;
        SaveSettings();
    }

    partial void OnCopyFileOnImportChanged(bool value)
    {
        _settings.CopyFileOnImport = value;
        SaveSettings();
        
        // 通知 RadioButton 绑定更新
        OnPropertyChanged(nameof(IsCopyFileOnImport));
        OnPropertyChanged(nameof(IsMoveFileOnImport));
    }
    
    partial void OnEnableAcrylicChanged(bool value)
    {
        _settings.EnableAcrylic = value;
        SaveSettings();
        
        // 立即应用毛玻璃效果
        ApplyAcrylicEffect(value);
    }
    
    private void ApplyAcrylicEffect(bool enable)
    {
        if (TopLevel is Window window)
        {
            // 在 Avalonia 中，毛玻璃效果通过 TransparencyLevel 实现
            window.TransparencyLevelHint = enable 
                ? new[] { WindowTransparencyLevel.AcrylicBlur }
                : new[] { WindowTransparencyLevel.None };
        }
    }
    
    partial void OnSelectedLanguageChanged(LanguageOption? value)
    {
        if (value != null && value.Code != _localization.CurrentLanguage)
        {
            _localization.CurrentLanguage = value.Code;
            _settings.Language = value.Code;
            SaveSettings();
            
            // 通知所有属性更新，确保界面刷新
            OnPropertyChanged(nameof(Localization));
            
            StatusMessage = _localization["Language"] + " " + _localization["FolderUpdated"];
        }
    }

    [RelayCommand]
    private async Task SelectFolderAsync()
    {
        if (TopLevel == null) return;

        var folders = await TopLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "选择安装包存储目录",
            AllowMultiple = false
        });

        if (folders.Count > 0)
        {
            var path = folders[0].Path.LocalPath;
            PackageRootPath = path;
            _settings.PackageRootPath = path;
            
            // 更新DataDirectoryService的PackageRootPath，这会触发数据目录的重建
            DataDirectoryService.PackageRootPath = path;
            
            SaveSettings();
            StatusMessage = "目录已更新，数据将保存到新目录";
        }
    }

    [RelayCommand]
    private void OpenFolder()
    {
        if (!string.IsNullOrEmpty(PackageRootPath) && Directory.Exists(PackageRootPath))
        {
            System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = PackageRootPath,
                UseShellExecute = true
            });
        }
    }

    [RelayCommand]
    private void ClearIconCache()
    {
        try
        {
            // 清理安装包图标缓存
            var packageIconService = new IconCacheService(DataDirectoryService.PackageIconCacheDirectory);
            packageIconService.ClearCache();
            
            // 清理已安装程序图标缓存
            var installedIconService = new IconCacheService(DataDirectoryService.InstalledIconCacheDirectory);
            installedIconService.ClearCache();
            
            StatusMessage = "图标缓存已清理";
        }
        catch (Exception ex)
        {
            StatusMessage = $"清理失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task ExportDataAsync()
    {
        if (TopLevel == null) return;

        try
        {
            var file = await TopLevel.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
            {
                Title = "导出数据",
                SuggestedFileName = $"ShibaPack_Data_{DateTime.Now:yyyyMMdd_HHmmss}.shibapack",
                FileTypeChoices = new[]
                {
                    new FilePickerFileType("ShibaPack Data")
                    {
                        Patterns = new[] { "*.shibapack" },
                        MimeTypes = new[] { "application/json" }
                    }
                }
            });

            if (file != null)
            {
                var success = await _importExportService.ExportDataAsync(file.Path.LocalPath);
                StatusMessage = success ? "数据导出成功" : "数据导出失败";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"导出失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task ImportDataAsync()
    {
        if (TopLevel == null) return;

        try
        {
            var files = await TopLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
            {
                Title = "导入数据",
                AllowMultiple = false,
                FileTypeFilter = new[]
                {
                    new FilePickerFileType("ShibaPack Data")
                    {
                        Patterns = new[] { "*.shibapack" },
                        MimeTypes = new[] { "application/json" }
                    }
                }
            });

            if (files.Count > 0)
            {
                var success = await _importExportService.ImportDataAsync(files[0].Path.LocalPath, overwrite: true);
                StatusMessage = success ? "数据导入成功，请重启应用" : "数据导入失败";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"导入失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task ExportFullBackupAsync()
    {
        if (TopLevel == null) return;

        try
        {
            var file = await TopLevel.StorageProvider.SaveFilePickerAsync(new FilePickerSaveOptions
            {
                Title = "导出完整备份",
                SuggestedFileName = $"ShibaPack_Backup_{DateTime.Now:yyyyMMdd_HHmmss}.zip",
                FileTypeChoices = new[]
                {
                    new FilePickerFileType("ZIP Archive")
                    {
                        Patterns = new[] { "*.zip" },
                        MimeTypes = new[] { "application/zip" }
                    }
                }
            });

            if (file != null)
            {
                var success = await _importExportService.ExportFullBackupAsync(file.Path.LocalPath);
                StatusMessage = success ? "完整备份导出成功" : "完整备份导出失败";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"导出失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task ImportFullBackupAsync()
    {
        if (TopLevel == null) return;

        try
        {
            var files = await TopLevel.StorageProvider.OpenFilePickerAsync(new FilePickerOpenOptions
            {
                Title = "导入完整备份",
                AllowMultiple = false,
                FileTypeFilter = new[]
                {
                    new FilePickerFileType("ZIP Archive")
                    {
                        Patterns = new[] { "*.zip" },
                        MimeTypes = new[] { "application/zip" }
                    }
                }
            });

            if (files.Count > 0)
            {
                var success = await _importExportService.ImportFullBackupAsync(files[0].Path.LocalPath);
                StatusMessage = success ? "完整备份导入成功，请重启应用" : "完整备份导入失败";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"导入失败: {ex.Message}";
        }
    }

    private void SaveSettings()
    {
        _settings.Save();
    }
}
