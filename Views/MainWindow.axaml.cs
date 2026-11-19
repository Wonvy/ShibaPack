using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using Avalonia.VisualTree;
using ShibaPack.Models;
using ShibaPack.ViewModels;

namespace ShibaPack.Views;

public partial class MainWindow : Window
{
    private WrapPanel? _gridViewWrapPanel;
    private readonly Dictionary<string, Avalonia.Media.IBrush?> _originalBackgrounds = new();
    
    public MainWindow()
    {
        InitializeComponent();
        
        // 设置拖拽事件
        AddHandler(DragDrop.DropEvent, OnDrop);
        AddHandler(DragDrop.DragOverEvent, OnDragOver);
    }

    protected override void OnLoaded(Avalonia.Interactivity.RoutedEventArgs e)
    {
        base.OnLoaded(e);
        
        // 设置TopLevel引用供ViewModel使用
        if (DataContext is MainWindowViewModel vm)
        {
            vm.TopLevel = this;
            
            // 订阅 GridItemWidth 变化事件
            vm.PropertyChanged += MainWindowViewModel_PropertyChanged;
            
            // 应用初始毛玻璃设置
            ApplyAcrylicEffect();
            
            // 查找 WrapPanel
            var gridViewListBox = this.FindControl<ListBox>("GridViewListBox");
            if (gridViewListBox != null)
            {
                // 等待布局完成后查找 WrapPanel
                Dispatcher.UIThread.Post(() =>
                {
                    _gridViewWrapPanel = gridViewListBox.GetVisualDescendants()
                        .OfType<WrapPanel>()
                        .FirstOrDefault();
                    
                    if (_gridViewWrapPanel != null && vm != null)
                    {
                        _gridViewWrapPanel.ItemWidth = vm.GridItemWidth;
                    }
                }, DispatcherPriority.Loaded);
            }
        }
        
        // 订阅窗口状态变化事件
        PropertyChanged += (s, e) =>
        {
            if (e.Property == Window.WindowStateProperty)
            {
                UpdateMaximizeButton(WindowState == WindowState.Maximized);
            }
        };
        
        // 初始化最大化按钮状态
        UpdateMaximizeButton(WindowState == WindowState.Maximized);
    }
    
    public void ApplyAcrylicEffect()
    {
        var settings = Models.AppSettings.Load();
        var enableAcrylic = settings.EnableAcrylic;
        var isDark = settings.ThemeMode == "Dark";
        
        TransparencyLevelHint = enableAcrylic 
            ? new[] { WindowTransparencyLevel.AcrylicBlur, WindowTransparencyLevel.Blur, WindowTransparencyLevel.Transparent }
            : new[] { WindowTransparencyLevel.None };
        
        // 窗口背景始终透明以支持毛玻璃效果
        Background = Avalonia.Media.Brushes.Transparent;
        
        // 关键区域背景改为半透明，否则会被不透明卡片背景“遮住模糊”
        var acrylicBrush = CreateAcrylicSurfaceBrush(isDark);
        SetBorderBackground("MainContainer", enableAcrylic, acrylicBrush);
        SetBorderBackground("TitleBar", enableAcrylic, acrylicBrush);
        SetBorderBackground("TopToolbar", enableAcrylic, acrylicBrush);
        SetBorderBackground("SearchBoxBorder", enableAcrylic, acrylicBrush);
        SetBorderBackground("LeftPane", enableAcrylic, acrylicBrush);
        SetBorderBackground("RightPane", enableAcrylic, acrylicBrush);
        SetBorderBackground("BottomBar", enableAcrylic, acrylicBrush);
    }
    
    private Avalonia.Media.IBrush CreateAcrylicSurfaceBrush(bool isDark)
    {
        // alpha 越低，背景透出越多，模糊越明显（但也更“透”）
        return new Avalonia.Media.SolidColorBrush(
            isDark
                ? Avalonia.Media.Color.FromArgb(160, 28, 28, 28)
                : Avalonia.Media.Color.FromArgb(210, 255, 255, 255));
    }
    
    private void SetBorderBackground(string controlName, bool enableAcrylic, Avalonia.Media.IBrush acrylicBrush)
    {
        var border = this.FindControl<Border>(controlName);
        if (border == null) return;
        
        if (!_originalBackgrounds.ContainsKey(controlName))
        {
            _originalBackgrounds[controlName] = border.Background;
        }
        
        border.Background = enableAcrylic ? acrylicBrush : _originalBackgrounds[controlName];
    }
    
    private void MinimizeButton_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        WindowState = WindowState.Minimized;
    }
    
    private void MaximizeButton_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (WindowState == WindowState.Maximized)
        {
            WindowState = WindowState.Normal;
        }
        else
        {
            WindowState = WindowState.Maximized;
        }
    }
    
    private void CloseButton_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        Close();
    }
    
    private void UpdateMaximizeButton(bool isMaximized)
    {
        var maximizeIcon = this.FindControl<Material.Icons.Avalonia.MaterialIcon>("MaximizeIcon");
        var restoreIcon = this.FindControl<Material.Icons.Avalonia.MaterialIcon>("RestoreIcon");
        
        if (maximizeIcon != null && restoreIcon != null)
        {
            maximizeIcon.IsVisible = !isMaximized;
            restoreIcon.IsVisible = isMaximized;
        }
    }
    
    private void TitleBar_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }
    
    /// <summary>
    /// 分类项：按下瞬间就选中（避免等待 PointerReleased / 后续筛选导致“卡顿感”）
    /// </summary>
    private void CategorySelectionBorder_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is not Control control) return;
        if (!e.GetCurrentPoint(control).Properties.IsLeftButtonPressed) return;
        
        // 立即选中对应的 TreeViewItem（TreeView 默认可能在释放时才选中）
        var item = control.FindAncestorOfType<TreeViewItem>();
        if (item != null)
        {
            item.IsSelected = true;
            item.Focus();
        }
        
        // 同步 ViewModel 选中项（确保绑定一致）
        if (control.DataContext is Category category && DataContext is MainWindowViewModel vm)
        {
            vm.SelectedCategory = category;
        }
        // 不标记 Handled，避免影响拖拽/右键菜单等交互
    }
    
    private void MainWindowViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(MainWindowViewModel.GridItemWidth) && sender is MainWindowViewModel vm)
        {
            if (_gridViewWrapPanel != null)
            {
                _gridViewWrapPanel.ItemWidth = vm.GridItemWidth;
            }
        }
    }

    private void ListViewItem_Tapped(object? sender, Avalonia.Input.TappedEventArgs e)
    {
        if (sender is Border border && border.DataContext is SoftwareGroup group)
        {
            if (DataContext is MainWindowViewModel vm)
            {
                vm.SelectedGroup = group;
            }
        }
    }

    private void GridViewItem_Tapped(object? sender, Avalonia.Input.TappedEventArgs e)
    {
        ListViewItem_Tapped(sender, e);
    }
    
    /// <summary>
    /// 网格视图项目双击事件处理
    /// </summary>
    private async void GridViewItem_DoubleTapped(object? sender, Avalonia.Input.TappedEventArgs e)
    {
        if (sender is Border border && border.DataContext is SoftwareGroup group)
        {
            if (DataContext is MainWindowViewModel vm && group.Packages.Count > 0)
            {
                var package = group.Packages[0]; // 使用最新版本
                
                // 检查文件是否存在
                if (string.IsNullOrEmpty(package.FullPath))
                    return;
                
                // 判断文件类型
                if (Directory.Exists(package.FullPath))
                {
                    // 如果是文件夹，打开文件夹
                    await vm.OpenInExplorerCommand.ExecuteAsync(package);
                }
                else if (File.Exists(package.FullPath))
                {
                    // 检查文件扩展名
                    var extension = Path.GetExtension(package.FullPath).ToLowerInvariant();
                    
                    if (extension == ".exe")
                    {
                        // exe文件直接执行
                        await vm.RunPackageCommand.ExecuteAsync(package);
                    }
                    else
                    {
                        // 其他文件类型（安装包等）也执行
                        await vm.RunPackageCommand.ExecuteAsync(package);
                    }
                }
            }
        }
    }
    
    /// <summary>
    /// 软件项按下时启动拖放
    /// </summary>
    private async void SoftwareItem_PointerPressed(object? sender, Avalonia.Input.PointerPressedEventArgs e)
    {
        if (sender is Border border && border.DataContext is SoftwareGroup group)
        {
            // 只在左键按下时启动拖放
            if (e.GetCurrentPoint(border).Properties.IsLeftButtonPressed)
            {
                // 使用 DataObject（虽然已过时，但在 Avalonia 11 中仍是最佳选择）
#pragma warning disable CS0618 // 类型或成员已过时
                var dragData = new DataObject();
                dragData.Set("SoftwareGroup", group);
                
                // 启动拖放操作
                await DragDrop.DoDragDrop(e, dragData, DragDropEffects.Move);
#pragma warning restore CS0618
            }
        }
    }
    
    /// <summary>
    /// 鼠标进入软件项时更新状态栏
    /// </summary>
    private void SoftwareItem_PointerEntered(object? sender, Avalonia.Input.PointerEventArgs e)
    {
        if (sender is Border border && border.DataContext is SoftwareGroup group)
        {
            if (DataContext is MainWindowViewModel vm)
            {
                // 构建详细信息
                var details = new List<string>
                {
                    $"版本数: {group.PackageCount}",
                    $"总大小: {group.TotalSizeText}",
                };
                
                if (!string.IsNullOrEmpty(group.PlatformsText))
                    details.Add($"平台: {group.PlatformsText}");
                
                if (!string.IsNullOrEmpty(group.Description))
                    details.Add($"描述: {group.Description}");
                
                vm.StatusMessage = string.Join(" | ", details);
            }
        }
    }
    
    /// <summary>
    /// 鼠标离开软件项时清空状态栏
    /// </summary>
    private void SoftwareItem_PointerExited(object? sender, Avalonia.Input.PointerEventArgs e)
    {
        if (DataContext is MainWindowViewModel vm)
        {
            vm.StatusMessage = string.Empty;
        }
    }
    
    /// <summary>
    /// 网格视图滚轮缩放处理
    /// </summary>
    private void GridViewListBox_PointerWheelChanged(object? sender, PointerWheelEventArgs e)
    {
        // 检查是否按下了 Ctrl 键
        if (e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            if (DataContext is MainWindowViewModel vm)
            {
                // 计算缩放增量（每次滚轮滚动调整10像素）
                var delta = e.Delta.Y > 0 ? -10.0 : 10.0;
                var newWidth = vm.GridItemWidth + delta;
                
                // 限制宽度范围（最小120，最大400）
                newWidth = Math.Max(120, Math.Min(400, newWidth));
                
                vm.GridItemWidth = newWidth;
                
                e.Handled = true; // 阻止默认滚动行为
            }
        }
    }

#pragma warning disable CS0618 // 类型或成员已过时
    private void OnDragOver(object? sender, DragEventArgs e)
    {
        // 检查是否拖拽到分类项上
        var targetCategory = GetCategoryFromDragEvent(e);
        if (targetCategory != null && e.Data.Contains("SoftwareGroup"))
        {
            e.DragEffects = DragDropEffects.Move;
            return;
        }
        
        e.DragEffects = e.Data.Contains(DataFormats.Files) 
            ? DragDropEffects.Copy 
            : DragDropEffects.None;
    }

    private async void OnDrop(object? sender, DragEventArgs e)
    {
        // 检查是否拖放到分类项上
        var targetCategory = GetCategoryFromDragEvent(e);
        if (targetCategory != null && e.Data.Contains("SoftwareGroup"))
        {
            var group = e.Data.Get("SoftwareGroup") as SoftwareGroup;
            if (group != null && DataContext is MainWindowViewModel vm)
            {
                await vm.MoveSoftwareToCategory(group, targetCategory);
            }
            return;
        }
        
        // 处理文件拖放
        var files = e.Data.GetFiles();
        if (files == null) return;

        var filePaths = new List<string>();
        foreach (var item in files)
        {
            if (item is IStorageFile file)
            {
                filePaths.Add(file.Path.LocalPath);
            }
            else if (item is IStorageFolder folder)
            {
                // 如果拖入的是文件夹，获取其中的文件
                await foreach (var subItem in folder.GetItemsAsync())
                {
                    if (subItem is IStorageFile subFile)
                    {
                        filePaths.Add(subFile.Path.LocalPath);
                    }
                }
            }
        }

        if (filePaths.Count > 0 && DataContext is MainWindowViewModel vm2)
        {
            await vm2.HandleDroppedFilesAsync(filePaths);
        }
    }
    
    /// <summary>
    /// 从拖放事件中获取目标分类
    /// </summary>
    private Category? GetCategoryFromDragEvent(DragEventArgs e)
    {
        if (e.Source is Control control)
        {
            // 向上查找 Border 并检查 Tag
            var current = control as Visual;
            while (current != null)
            {
                if (current is Border border && border.Tag is Category category)
                {
                    return category;
                }
                current = current.GetVisualParent();
            }
        }
        return null;
    }
#pragma warning restore CS0618
}
