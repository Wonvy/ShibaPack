using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ShibaPack.Models;
using ShibaPack.Services;

namespace ShibaPack.ViewModels;

public partial class MainWindowViewModel : ViewModelBase
{
    private readonly PackageManagerService _packageManager = new();
    private readonly IconCacheService _installedIconCache = new(DataDirectoryService.InstalledIconCacheDirectory);
    private readonly ThemeService _themeService = new();
    private readonly LocalizationService _localization = LocalizationService.Instance;
    private readonly CategoryService _categoryService;
    private readonly FileVersionInfoService _versionInfoService = new();
    private readonly InstalledProgramService _installedProgramService;
    private readonly AppSettings _settings;
    private readonly PlatformExtensionService _platformExtensionService = new();
    
    private List<InstalledProgram> _installedPrograms = [];

    [ObservableProperty]
    private string _packageRootPath = string.Empty;

    [ObservableProperty]
    private Category? _selectedCategory;

    [ObservableProperty]
    private SoftwareGroup? _selectedGroup;

    [ObservableProperty]
    private PackageInfo? _selectedPackage;

    [ObservableProperty]
    private string _searchText = string.Empty;

    [ObservableProperty]
    private string _statusMessage = "就绪";

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isDragOver;

    [ObservableProperty]
    private bool _isListView = true;
    
    [RelayCommand]
    private void SetListView(string isListViewStr)
    {
        IsListView = isListViewStr == "True";
    }
    
    [RelayCommand]
    private void SetPlatformFilter(string indexStr)
    {
        if (int.TryParse(indexStr, out var index))
        {
            PlatformFilterIndex = index;
        }
    }
    
    [ObservableProperty]
    private double _gridItemWidth = 190.0; // 网格视图项目宽度，默认190

    [ObservableProperty]
    private double _nameColumnWidth = 300.0; // 软件名称列宽度（增加初始宽度）
    
    [ObservableProperty]
    private double _versionColumnWidth = 100.0; // 版本列宽度（固定宽度）
    
    [ObservableProperty]
    private double _platformColumnWidth = 120.0; // 平台列宽度
    
    [ObservableProperty]
    private double _sizeColumnWidth = 120.0; // 文件大小列宽度
    
    [ObservableProperty]
    private double _modifiedTimeColumnWidth = 180.0; // 修改时间列宽度

    [ObservableProperty]
    private int _platformFilterIndex = 0;

    [ObservableProperty]
    private int _sortOrderIndex = 0; // 保留用于兼容，但不再使用
    
    [ObservableProperty]
    private string _sortColumn = "Name"; // Name, Size, ModifiedTime
    
    [ObservableProperty]
    private bool _sortAscending = true; // true=升序, false=降序
    
    [ObservableProperty]
    private bool _showInstalledPrograms = false;
    
    [ObservableProperty]
    private string _installedSortColumn = "Name"; // Name, Size, InstallDate
    
    [ObservableProperty]
    private bool _installedSortAscending = true; // true=升序, false=降序
    
    public ObservableCollection<InstalledProgram> InstalledPrograms { get; } = new();
    
    // 计算属性：是否显示安装包列表
    public bool ShowPackageList => !ShowInstalledPrograms;
    
    partial void OnShowInstalledProgramsChanged(bool value)
    {
        OnPropertyChanged(nameof(ShowPackageList));
    }

    public ObservableCollection<Category> Categories => _categoryService.Categories;
    
    public ObservableCollection<SoftwareGroup> FilteredGroups { get; } = [];

    public int TotalPackageCount => _packageManager.AllPackages.Count;
    
    public int TotalSoftwareCount => _packageManager.SoftwareGroups.Count;

    // 用于存储窗口引用以获取 StorageProvider
    public Avalonia.Controls.TopLevel? TopLevel { get; set; }
    
    public LocalizationService Localization => _localization;

    public MainWindowViewModel()
    {
        _installedProgramService = new InstalledProgramService(_installedIconCache, _versionInfoService);
        _settings = AppSettings.Load();
        
        // 先设置 DataDirectoryService.PackageRootPath，再初始化 CategoryService
        if (!string.IsNullOrEmpty(_settings.PackageRootPath))
        {
            DataDirectoryService.PackageRootPath = _settings.PackageRootPath;
        }
        
        // 现在初始化 CategoryService，此时路径已设置
        _categoryService = new CategoryService();
        _categoryService.CategoriesChanged += OnCategoriesChanged;
        
        PackageRootPath = _settings.PackageRootPath;

        // 初始化主题
        var isDark = _settings.ThemeMode == "Dark";
        _themeService.SetTheme(isDark ? ThemeMode.Dark : ThemeMode.Light);
        IsDarkMode = isDark;
        
        // 初始化语言
        _localization.CurrentLanguage = _settings.Language;

        _packageManager.DataChanged += OnDataChanged;
        _categoryService.CategoriesChanged += OnCategoriesChanged;

        // 初始化加载
        _ = InitializeAsync();
    }

    [ObservableProperty]
    private bool isDarkMode = true;

    private void OnCategoriesChanged()
    {
        _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
        OnPropertyChanged(nameof(Categories));
    }

    private async Task InitializeAsync()
    {
        await _packageManager.LoadDataAsync();
        
        // 应用保存的软件分类映射
        var categoryMappings = _categoryService.LoadSoftwareCategoryMappings();
        _packageManager.ApplyCategoryMappings(categoryMappings);
        
        // 加载已安装程序
        await LoadInstalledProgramsAsync();
        
        if (!string.IsNullOrEmpty(PackageRootPath) && Directory.Exists(PackageRootPath))
        {
            await RefreshAsync();
        }
        
        // 更新分类计数和列表
        _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
        UpdateFilteredGroups();
    }

    private void OnDataChanged()
    {
        OnPropertyChanged(nameof(TotalPackageCount));
        OnPropertyChanged(nameof(TotalSoftwareCount));
        _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
        
        // 调试：输出分类数量
        foreach (var category in Categories)
        {
            System.Diagnostics.Debug.WriteLine($"分类: {category.Name}, 数量: {category.SoftwareCount}");
        }
        
        UpdateFilteredGroups();
    }

    partial void OnSelectedCategoryChanged(Category? value)
    {
        // 检查是否选中"已安装"分类（异步设置，不阻塞UI）
        var isInstalled = value?.Id == "installed";
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            ShowInstalledPrograms = isInstalled;
        }, Avalonia.Threading.DispatcherPriority.Background);
        
        // 异步更新筛选，让 UI 立即响应高亮
        _ = Task.Run(async () =>
        {
            // 重要：不要在后台线程直接枚举 ObservableCollection（可能跨线程/导致卡顿/异常）
            // 先在 UI 线程快速快照，再后台计算
            List<SoftwareGroup> allGroupsSnapshot = [];
            var searchSnapshot = SearchText;
            var platformIndexSnapshot = PlatformFilterIndex;
            var sortColumnSnapshot = SortColumn;
            var sortAscendingSnapshot = SortAscending;
            
            await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
            {
                allGroupsSnapshot = _packageManager.SoftwareGroups.ToList();
            });

            IEnumerable<SoftwareGroup> filtered;
            
            if (value == null || value.Id == "all")
            {
                filtered = allGroupsSnapshot;
            }
            else if (value.Id == "uncategorized")
            {
                filtered = allGroupsSnapshot.Where(g => g.CategoryId == "uncategorized");
            }
            else
            {
                filtered = allGroupsSnapshot.Where(g => g.CategoryId == value.Id);
            }
            
            // 根据搜索文本筛选
            if (!string.IsNullOrWhiteSpace(searchSnapshot))
            {
                filtered = filtered.Where(g => 
                    g.Name.Contains(searchSnapshot, StringComparison.OrdinalIgnoreCase) ||
                    g.Packages.Any(p => p.FileName.Contains(searchSnapshot, StringComparison.OrdinalIgnoreCase)));
            }
            
            // 根据平台筛选
            if (platformIndexSnapshot > 0)
            {
                var platformFilter = platformIndexSnapshot switch
                {
                    1 => PlatformType.Windows,
                    2 => PlatformType.MacOS,
                    3 => PlatformType.Linux,
                    _ => (PlatformType?)null
                };
                
                if (platformFilter != null)
                {
                    filtered = filtered.Where(g => 
                        g.Packages.Any(p => p.Platform == platformFilter || p.Platform == PlatformType.CrossPlatform));
                }
            }
            
            // 排序
            filtered = sortColumnSnapshot switch
            {
                "Name" => sortAscendingSnapshot 
                    ? filtered.OrderBy(g => g.Name) 
                    : filtered.OrderByDescending(g => g.Name),
                "Size" => sortAscendingSnapshot 
                    ? filtered.OrderBy(g => g.TotalSize) 
                    : filtered.OrderByDescending(g => g.TotalSize),
                "ModifiedTime" => sortAscendingSnapshot 
                    ? filtered.OrderBy(g => g.Packages.Max(p => p.ModifiedTime)) 
                    : filtered.OrderByDescending(g => g.Packages.Max(p => p.ModifiedTime)),
                _ => filtered.OrderBy(g => g.Name)
            };
            
            var filteredList = filtered.ToList();
            
            // 回到UI线程更新集合
            await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(() =>
            {
                FilteredGroups.Clear();
                foreach (var group in filteredList)
                {
                    FilteredGroups.Add(group);
                }
                
                if (SelectedGroup == null || !FilteredGroups.Contains(SelectedGroup))
                {
                    SelectedGroup = FilteredGroups.FirstOrDefault();
                }
            });
        });
    }
    
        partial void OnSearchTextChanged(string value)
    {
        UpdateFilteredGroups();
    }
    
    partial void OnPlatformFilterIndexChanged(int value)
    {
        UpdateFilteredGroups();
    }
    
    partial void OnSortOrderIndexChanged(int value)
    {
        UpdateFilteredGroups();
    }
    
    partial void OnSortColumnChanged(string value)
    {
        UpdateFilteredGroups();
    }
    
    partial void OnSortAscendingChanged(bool value)
    {
        UpdateFilteredGroups();
    }
    
    [RelayCommand]
    private void SortByColumn(string column)
    {
        if (SortColumn == column)
        {
            // 如果点击的是当前排序列，切换排序方向
            SortAscending = !SortAscending;
        }
        else
        {
            // 如果点击的是新列，设置为升序
            SortColumn = column;
            SortAscending = true;
        }
    }
    
    partial void OnInstalledSortColumnChanged(string value)
    {
        SortInstalledPrograms();
    }
    
    partial void OnInstalledSortAscendingChanged(bool value)
    {
        SortInstalledPrograms();
    }
    
    [RelayCommand]
    private void SortInstalledPrograms(string column)
    {
        if (InstalledSortColumn == column)
        {
            // 如果点击的是当前排序列，切换排序方向
            InstalledSortAscending = !InstalledSortAscending;
        }
        else
        {
            // 如果点击的是新列，设置为升序
            InstalledSortColumn = column;
            InstalledSortAscending = true;
        }
        SortInstalledPrograms();
    }
    
    private void SortInstalledPrograms()
    {
        var sortedList = InstalledSortColumn switch
        {
            "Name" => InstalledSortAscending
                ? InstalledPrograms.OrderBy(p => p.DisplayName).ToList()
                : InstalledPrograms.OrderByDescending(p => p.DisplayName).ToList(),
            "Size" => InstalledSortAscending
                ? InstalledPrograms.OrderBy(p => p.EstimatedSize).ToList()
                : InstalledPrograms.OrderByDescending(p => p.EstimatedSize).ToList(),
            "InstallDate" => InstalledSortAscending
                ? InstalledPrograms.OrderBy(p => p.InstallDate ?? DateTime.MinValue).ToList()
                : InstalledPrograms.OrderByDescending(p => p.InstallDate ?? DateTime.MinValue).ToList(),
            _ => InstalledPrograms.ToList()
        };
        
        InstalledPrograms.Clear();
        foreach (var program in sortedList)
        {
            InstalledPrograms.Add(program);
        }
    }

    partial void OnSelectedGroupChanged(SoftwareGroup? value)
    {
        SelectedPackage = value?.Packages.FirstOrDefault();
    }

    private void UpdateFilteredGroups()
    {
        FilteredGroups.Clear();
        
        var allGroups = _packageManager.SoftwareGroups;
        // Debug 输出在大列表时会明显拖慢 UI（尤其是频繁筛选/切换）
#if DEBUG
        System.Diagnostics.Debug.WriteLine($"[UpdateFilteredGroups] 全部分组数: {allGroups.Count}, 当前分类: {SelectedCategory?.Id ?? "null"}");
#endif
        
        // 根据选中的分类筛选
        IEnumerable<SoftwareGroup> filtered;
        if (SelectedCategory == null || SelectedCategory.Id == "all")
        {
            filtered = allGroups;
        }
        else if (SelectedCategory.Id == "uncategorized")
        {
            filtered = allGroups.Where(g => g.CategoryId == "uncategorized");
        }
        else
        {
            filtered = allGroups.Where(g => g.CategoryId == SelectedCategory.Id);
        }
        
        // 根据搜索文本筛选
        if (!string.IsNullOrWhiteSpace(SearchText))
        {
            filtered = filtered.Where(g => 
                g.Name.Contains(SearchText, StringComparison.OrdinalIgnoreCase) ||
                g.Packages.Any(p => p.FileName.Contains(SearchText, StringComparison.OrdinalIgnoreCase)));
        }
        
        // 根据平台筛选
        if (PlatformFilterIndex > 0)
        {
            var platformFilter = PlatformFilterIndex switch
            {
                1 => PlatformType.Windows,
                2 => PlatformType.MacOS,
                3 => PlatformType.Linux,
                _ => (PlatformType?)null
            };
            
            if (platformFilter != null)
            {
                filtered = filtered.Where(g => 
                    g.Packages.Any(p => p.Platform == platformFilter || p.Platform == PlatformType.CrossPlatform));
            }
        }
        
        // 排序
        filtered = SortColumn switch
        {
            "Name" => SortAscending 
                ? filtered.OrderBy(g => g.Name) 
                : filtered.OrderByDescending(g => g.Name),
            "Size" => SortAscending 
                ? filtered.OrderBy(g => g.TotalSize) 
                : filtered.OrderByDescending(g => g.TotalSize),
            "ModifiedTime" => SortAscending 
                ? filtered.OrderBy(g => g.Packages.Max(p => p.ModifiedTime)) 
                : filtered.OrderByDescending(g => g.Packages.Max(p => p.ModifiedTime)),
            _ => filtered.OrderBy(g => g.Name)
        };

        foreach (var group in filtered)
        {
            FilteredGroups.Add(group);
        }
        
#if DEBUG
        System.Diagnostics.Debug.WriteLine($"[UpdateFilteredGroups] 过滤后分组数: {FilteredGroups.Count}");
#endif

        if (SelectedGroup == null || !FilteredGroups.Contains(SelectedGroup))
        {
            SelectedGroup = FilteredGroups.FirstOrDefault();
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
            _settings.Save();

            await RefreshAsync();
            
            StatusMessage = $"已设置目录: {path}";
        }
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        if (string.IsNullOrEmpty(PackageRootPath)) return;

        IsLoading = true;
        StatusMessage = "正在扫描...";
        
        try
        {
            await _packageManager.ScanDirectoryAsync(PackageRootPath);
            
            // 重新应用保存的软件分类映射（扫描会清空分类信息）
            var categoryMappings = _categoryService.LoadSoftwareCategoryMappings();
            _packageManager.ApplyCategoryMappings(categoryMappings);
            
            // ScanDirectoryAsync 会触发 DataChanged 事件，OnDataChanged() 会自动更新分类计数和刷新UI
            StatusMessage = $"扫描完成，共 {TotalPackageCount} 个安装包";
        }
        catch (Exception ex)
        {
            StatusMessage = $"扫描失败: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    private async Task OrganizeAsync()
    {
        if (string.IsNullOrEmpty(PackageRootPath)) return;

        IsLoading = true;
        StatusMessage = "正在整理...";
        
        try
        {
            await _packageManager.OrganizePackagesAsync(PackageRootPath);
            StatusMessage = "整理完成";
        }
        catch (Exception ex)
        {
            StatusMessage = $"整理失败: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    private async Task DeletePackageAsync(PackageInfo? package)
    {
        if (package == null) return;

        try
        {
            await _packageManager.DeletePackageAsync(package);
            StatusMessage = $"已删除: {package.FileName}";
        }
        catch (Exception ex)
        {
            StatusMessage = $"删除失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task CleanOldVersionsAsync(SoftwareGroup? group)
    {
        if (group == null || group.Packages.Count <= 1) return;

        try
        {
            await _packageManager.CleanOldVersionsAsync(group, 1);
            StatusMessage = $"已清理 {group.Name} 的旧版本";
        }
        catch (Exception ex)
        {
            StatusMessage = $"清理失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task OpenInExplorer(PackageInfo? package)
    {
        if (package == null) return;
        await Task.Run(() => _packageManager.OpenInExplorer(package.FullPath));
    }

    [RelayCommand]
    private async Task OpenRootFolder()
    {
        if (!string.IsNullOrEmpty(PackageRootPath))
        {
            await Task.Run(() => _packageManager.OpenInExplorer(PackageRootPath));
        }
    }

    [RelayCommand]
    private void OpenSettings()
    {
        if (TopLevel is not Avalonia.Controls.Window window)
            return;
            
        var settingsWindow = new Views.SettingsWindow
        {
            DataContext = new SettingsViewModel()
        };
        settingsWindow.ShowDialog(window);
        
        // 设置窗口关闭后重新加载设置
        settingsWindow.Closed += (s, e) =>
        {
            var newSettings = AppSettings.Load();
            if (newSettings.PackageRootPath != PackageRootPath)
            {
                PackageRootPath = newSettings.PackageRootPath;
                _ = RefreshAsync();
            }
            
            // 应用毛玻璃效果到主窗口
            if (TopLevel is Avalonia.Controls.Window mainWindow)
            {
                mainWindow.TransparencyLevelHint = newSettings.EnableAcrylic 
                    ? new[] { Avalonia.Controls.WindowTransparencyLevel.AcrylicBlur }
                    : new[] { Avalonia.Controls.WindowTransparencyLevel.None };
            }
        };
    }

    [RelayCommand]
    private Task SetSoftwareCategoryAsync(SoftwareGroup? software)
    {
        if (software == null) return Task.CompletedTask;
        // TODO: 显示分类选择对话框
        return Task.CompletedTask;
    }
    
    /// <summary>
    /// 将软件移动到指定分类
    /// </summary>
    public async Task MoveSoftwareToCategory(SoftwareGroup software, Category targetCategory)
    {
        if (software == null || targetCategory == null) return;
        
        // 不能移动到 "all" 或 "installed" 分类
        if (targetCategory.Id == "all" || targetCategory.Id == "installed")
        {
            StatusMessage = "无法移动到该分类";
            return;
        }
        
        var oldCategoryId = software.CategoryId;
        
        // 如果目标分类和当前分类相同，不需要移动
        if (oldCategoryId == targetCategory.Id)
        {
            return;
        }
        
        // 更新软件的分类ID（从旧分类移除，添加到新分类）
        software.CategoryId = targetCategory.Id;
        
        // 保存分类关联
        await _categoryService.SaveSoftwareCategoryAsync(software.Name, targetCategory.Id);
        
        // 更新分类计数（这会自动更新所有分类的计数，包括旧分类和新分类）
        _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
        
        // 更新过滤列表（确保从当前视图的列表中移除或添加）
        UpdateFilteredGroups();
        
        StatusMessage = $"已将 {software.Name} 从 {GetCategoryName(oldCategoryId)} 移动到 {targetCategory.Name}";
    }
    
    /// <summary>
    /// 根据分类ID获取分类名称
    /// </summary>
    private string GetCategoryName(string categoryId)
    {
        if (categoryId == "all") return "所有软件";
        if (categoryId == "installed") return "已安装";
        if (categoryId == "uncategorized") return "未分类";
        
        var category = _categoryService.GetCategoryById(categoryId);
        return category?.Name ?? "未知分类";
    }

    [RelayCommand]
    private async Task AddCategoryAsync()
    {
        if (TopLevel is not Avalonia.Controls.Window window)
            return;
        
        // 显示输入对话框
        var nameDialog = new Views.InputDialog("添加分类", "请输入分类名称:", "");
        await nameDialog.ShowDialog(window);
        
        if (!nameDialog.IsConfirmed || string.IsNullOrWhiteSpace(nameDialog.InputText))
            return;
        
        var categoryName = nameDialog.InputText.Trim();
        
        try
        {
            // 使用默认图标
            var iconName = "Package";
            var category = await _categoryService.AddCategoryAsync(categoryName, iconName);
            
            if (category != null)
            {
                _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
                
                // 选中新添加的分类
                SelectedCategory = category;
                
                StatusMessage = $"已添加分类: {categoryName}";
            }
        }
        catch (InvalidOperationException ex)
        {
            StatusMessage = ex.Message;
        }
        catch (Exception ex)
        {
            StatusMessage = $"添加分类失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task ResetCategoriesAsync()
    {
        if (TopLevel is not Avalonia.Controls.Window window)
            return;
        
        // 显示确认对话框
        var dialog = new Views.ConfirmDialog("重置分类", "确定要重置为默认分类吗？此操作将删除所有自定义分类。");
        await dialog.ShowDialog(window);
        
        if (dialog.IsConfirmed)
        {
            await _categoryService.ResetToDefaultCategoriesAsync();
            _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
            StatusMessage = "已重置为默认分类";
        }
    }

    [RelayCommand]
    private async Task AddSubCategoryAsync(Category? parentCategory)
    {
        if (parentCategory == null || TopLevel is not Avalonia.Controls.Window window)
            return;
        
        // 显示输入对话框
        var nameDialog = new Views.InputDialog("添加子分类", "请输入子分类名称:", "");
        await nameDialog.ShowDialog(window);
        
        if (!nameDialog.IsConfirmed || string.IsNullOrWhiteSpace(nameDialog.InputText))
            return;
        
        var categoryName = nameDialog.InputText.Trim();
        
        try
        {
            // 使用默认图标
            var iconName = "Package";
            var category = await _categoryService.AddCategoryAsync(categoryName, iconName, parentCategory.Id);
            
            if (category != null)
            {
                // 自动展开父分类，让用户能看到新添加的子分类
                parentCategory.IsExpanded = true;
                
                _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
                
                // 等待 UI 更新，然后选中新添加的子分类
                await Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(async () =>
                {
                    await Task.Delay(50);
                    SelectedCategory = category;
                });
                
                StatusMessage = $"已添加子分类: {categoryName}";
            }
        }
        catch (InvalidOperationException ex)
        {
            StatusMessage = ex.Message;
        }
        catch (Exception ex)
        {
            StatusMessage = $"添加子分类失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private async Task EditCategoryAsync(Category? category)
    {
        if (category == null || category.Id == "all" || category.Id == "uncategorized" || category.Id == "installed")
        {
            StatusMessage = "系统分类不能编辑";
            return;
        }
        
        if (TopLevel is not Avalonia.Controls.Window window)
            return;
        
        var dialog = new Views.InputDialog("编辑分类", "请输入新的分类名称:", category.Name);
        await dialog.ShowDialog(window);
        
        if (dialog.IsConfirmed && !string.IsNullOrWhiteSpace(dialog.InputText))
        {
            var newName = dialog.InputText.Trim();
            if (newName != category.Name)
            {
                var oldName = category.Name;
                try
                {
                    category.Name = newName;
                    await _categoryService.UpdateCategoryAsync(category);
                    _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
                    StatusMessage = $"已重命名: {oldName} → {newName}";
                }
                catch (InvalidOperationException ex)
                {
                    // 恢复原名称
                    category.Name = oldName;
                    StatusMessage = ex.Message;
                }
                catch (Exception ex)
                {
                    // 恢复原名称
                    category.Name = oldName;
                    StatusMessage = $"重命名失败: {ex.Message}";
                }
            }
        }
    }

    [RelayCommand]
    private async Task ChangeCategoryIconAsync(Category? category)
    {
        if (category == null || TopLevel is not Avalonia.Controls.Window window)
            return;
        
        var iconPicker = new Views.IconPickerWindow();
        
        // 设置预览分类，让图标选择时实时预览
        iconPicker.SetPreviewCategory(category);
        
        // 如果有当前图标，设置为初始选中
        if (!string.IsNullOrEmpty(category.Icon))
        {
            // 尝试将字符串转换为 MaterialIconKind
            if (Enum.TryParse<Material.Icons.MaterialIconKind>(category.Icon, out var currentIcon))
            {
                iconPicker.SetInitialIcon(currentIcon);
            }
        }
        
        await iconPicker.ShowDialog(window);
        
        var selectedIcon = iconPicker.GetSelectedIcon();
        if (selectedIcon.HasValue)
        {
            // 确认时才保存（预览已经在选择时更新了）
            await _categoryService.UpdateCategoryAsync(category);
            StatusMessage = $"已更换图标: {category.Name}";
        }
        // 取消时图标已经通过 RestoreOriginalIcon 还原了
    }

    [RelayCommand]
    private async Task DeleteCategoryAsync(Category? category)
    {
        if (category == null) return;
        
        // 防止删除系统分类
        if (category.Id == "all" || category.Id == "uncategorized")
        {
            StatusMessage = "系统分类不能删除";
            return;
        }
        
        var success = await _categoryService.DeleteCategoryAsync(category.Id);
        if (success)
        {
            // 将该分类下的软件移动到未分类
            foreach (var software in _packageManager.SoftwareGroups.Where(s => s.CategoryId == category.Id))
            {
                software.CategoryId = "uncategorized";
            }
            _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
            StatusMessage = $"已删除分类: {category.Name}";
        }
    }

    /// <summary>
    /// 处理拖拽放入的文件
    /// </summary>
    public async Task HandleDroppedFilesAsync(IEnumerable<string> files)
    {
        if (string.IsNullOrEmpty(PackageRootPath))
        {
            StatusMessage = "请先设置安装包存储目录";
            return;
        }

        // 确定目标分类：如果当前选中的是实际分类，就用它；否则用 uncategorized
        var targetCategoryId = SelectedCategory?.Id;
        if (string.IsNullOrEmpty(targetCategoryId) || targetCategoryId == "all" || targetCategoryId == "installed")
        {
            targetCategoryId = "uncategorized";
        }

        var count = 0;
        var skippedCount = 0;
        var importedGroups = new List<SoftwareGroup>();
        var skippedFiles = new List<string>();
        
        foreach (var file in files)
        {
            try
            {
                // 检查是否为文件夹
                string? platformFolder = null;
                if (_platformExtensionService.IsDirectory(file))
                {
                    // 文件夹需要用户选择平台
                    if (TopLevel is Avalonia.Controls.Window window)
                    {
                        var dialog = new Views.PlatformSelectDialog(Path.GetFileName(file));
                        await dialog.ShowDialog(window);
                        if (dialog.IsConfirmed && !string.IsNullOrEmpty(dialog.SelectedPlatform))
                        {
                            platformFolder = dialog.SelectedPlatform;
                            if (dialog.RememberChoice)
                            {
                                // 文件夹没有后缀，不保存到配置
                            }
                        }
                        else
                        {
                            // 用户取消，跳过此文件
                            skippedCount++;
                            skippedFiles.Add(Path.GetFileName(file));
                            continue;
                        }
                    }
                }
                else
                {
                    // 根据文件后缀识别平台
                    var platform = _platformExtensionService.GetPlatformByExtension(file);
                    if (string.IsNullOrEmpty(platform))
                    {
                        // 后缀未识别，弹出对话框让用户选择
                        if (TopLevel is Avalonia.Controls.Window window)
                        {
                            var ext = Path.GetExtension(file).TrimStart('.');
                            var dialog = new Views.PlatformSelectDialog(Path.GetFileName(file));
                            await dialog.ShowDialog(window);
                            if (dialog.IsConfirmed && !string.IsNullOrEmpty(dialog.SelectedPlatform))
                            {
                                platform = dialog.SelectedPlatform;
                                platformFolder = platform;
                                if (dialog.RememberChoice && !string.IsNullOrEmpty(ext))
                                {
                                    // 保存用户选择到配置
                                    _platformExtensionService.AddOrUpdateExtension(ext, platform);
                                }
                            }
                            else
                            {
                                // 用户取消，跳过此文件
                                skippedCount++;
                                skippedFiles.Add(Path.GetFileName(file));
                                continue;
                            }
                        }
                    }
                    else
                    {
                        platformFolder = platform;
                    }
                }
                
                var result = await _packageManager.AddPackageAsync(file, PackageRootPath, platformFolder);
                if (result != null)
                {
                    count++;
                    // 找到新导入软件所属的分组
                    var group = _packageManager.SoftwareGroups.FirstOrDefault(g => 
                        g.Name.Equals(result.SoftwareName, StringComparison.OrdinalIgnoreCase));
                    if (group != null && !importedGroups.Contains(group))
                    {
                        importedGroups.Add(group);
                    }
                }
            }
            catch (InvalidOperationException ex)
            {
                // 文件已存在，跳过
                skippedCount++;
                skippedFiles.Add(Path.GetFileName(file));
                System.Diagnostics.Debug.WriteLine($"[HandleDroppedFiles] 跳过重复文件: {file}, 原因: {ex.Message}");
            }
            catch (Exception ex)
            {
                // 其他错误也跳过
                skippedCount++;
                skippedFiles.Add(Path.GetFileName(file));
                System.Diagnostics.Debug.WriteLine($"[HandleDroppedFiles] 处理文件失败: {file}, 错误: {ex.Message}");
            }
        }

        if (count > 0)
        {
            // 将新导入的软件分配到目标分类
            foreach (var group in importedGroups)
            {
                group.CategoryId = targetCategoryId;
                await _categoryService.SaveSoftwareCategoryAsync(group.Name, targetCategoryId);
            }
            
            // 更新分类计数和刷新列表
            _categoryService.UpdateCategoryCounts(_packageManager.SoftwareGroups);
            UpdateFilteredGroups();
            
            var message = $"已导入 {count} 个安装包到 {SelectedCategory?.Name ?? "未分类"}";
            if (skippedCount > 0)
            {
                message += $"，跳过 {skippedCount} 个重复文件";
                if (skippedFiles.Count > 0 && skippedFiles.Count <= 5)
                {
                    message += $" ({string.Join(", ", skippedFiles)})";
                }
            }
            StatusMessage = message;
        }
        else
        {
            if (skippedCount > 0)
            {
                var message = $"所有文件都已存在，跳过 {skippedCount} 个重复文件";
                if (skippedFiles.Count > 0 && skippedFiles.Count <= 5)
                {
                    message += $" ({string.Join(", ", skippedFiles)})";
                }
                StatusMessage = message;
            }
            else
            {
                StatusMessage = "未找到有效的安装包文件";
            }
        }
    }
    
    /// <summary>
    /// 加载已安装程序
    /// </summary>
    private async Task LoadInstalledProgramsAsync()
    {
        try
        {
            InstalledPrograms.Clear();
            var programs = await _installedProgramService.GetInstalledProgramsAsync();
                
            foreach (var program in programs)
            {
                InstalledPrograms.Add(program);
            }
            
            // 加载后应用排序
            SortInstalledPrograms();
                
            // 更新"已安装"分类的计数
            var installedCategory = Categories.FirstOrDefault(c => c.Id == "installed");
            if (installedCategory != null)
            {
                installedCategory.SoftwareCount = InstalledPrograms.Count;
                System.Diagnostics.Debug.WriteLine($"已安装分类数量更新: {installedCategory.SoftwareCount}");
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"加载已安装程序失败: {ex.Message}";
        }
    }
    
    /// <summary>
    /// 运行安装包
    /// </summary>
    [RelayCommand]
    private async Task RunPackage(PackageInfo? package)
    {
        if (package == null || string.IsNullOrEmpty(package.FullPath)) return;
        
        try
        {
            await Task.Run(() =>
            {
                var startInfo = new System.Diagnostics.ProcessStartInfo
                {
                    FileName = package.FullPath,
                    UseShellExecute = true
                };
                System.Diagnostics.Process.Start(startInfo);
            });
            
            StatusMessage = $"已启动: {package.SoftwareName}";
        }
        catch (Exception ex)
        {
            StatusMessage = $"启动失败: {ex.Message}";
        }
    }
    
    /// <summary>
    /// 在资源管理器中显示文件
    /// </summary>
    [RelayCommand]
    private async Task ShowInExplorer(PackageInfo? package)
    {
        if (package == null || string.IsNullOrEmpty(package.FullPath)) return;
        
        try
        {
            if (File.Exists(package.FullPath))
            {
                await Task.Run(() =>
                {
                    System.Diagnostics.Process.Start("explorer.exe", $"/select,\"{package.FullPath}\"");
                });
                
                StatusMessage = $"已定位文件: {package.FileName}";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"显示文件失败: {ex.Message}";
        }
    }

    [RelayCommand]
    private void ToggleTheme()
    {
        IsDarkMode = !IsDarkMode;
        var newTheme = IsDarkMode ? ThemeMode.Dark : ThemeMode.Light;
        _themeService.SetTheme(newTheme);
        
        _settings.ThemeMode = IsDarkMode ? "Dark" : "Light";
        _settings.Save();
        
        // 调试输出
        var currentVariant = Avalonia.Application.Current?.ActualThemeVariant;
        var requestedVariant = Avalonia.Application.Current?.RequestedThemeVariant;
        StatusMessage = $"主题已切换: {(IsDarkMode ? "暗色" : "亮色")} | Current: {currentVariant?.Key} | Requested: {requestedVariant?.Key}";
    }

    [RelayCommand]
    private void SelectSoftware(SoftwareGroup? group)
    {
        if (group != null)
        {
            SelectedGroup = group;
        }
    }
    
    /// <summary>
    /// 卸载已安装程序
    /// </summary>
    [RelayCommand]
    private async Task UninstallProgram(InstalledProgram? program)
    {
        if (program == null)
        {
            StatusMessage = "卸载失败：程序为空";
            return;
        }
        
        try
        {
            // 直接启动卸载程序
            if (!string.IsNullOrEmpty(program.UninstallString))
            {
                StatusMessage = $"正在卸载: {program.DisplayName}";
                
                await Task.Run(() =>
                {
                    var startInfo = new System.Diagnostics.ProcessStartInfo
                    {
                        FileName = program.UninstallString,
                        UseShellExecute = true
                    };
                    
                    System.Diagnostics.Process.Start(startInfo);
                });
                
                StatusMessage = $"已启动卸载程序: {program.DisplayName}";
                
                // 等待一段时间后刷新列表
                await Task.Delay(2000);
                await LoadInstalledProgramsAsync();
            }
            else
            {
                StatusMessage = "未找到卸载程序";
            }
        }
        catch (Exception ex)
        {
            StatusMessage = $"卸载失败: {ex.Message}";
        }
    }
}

