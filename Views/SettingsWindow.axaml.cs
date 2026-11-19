using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;
using Avalonia.VisualTree;
using ShibaPack.Services;
using ShibaPack.ViewModels;

namespace ShibaPack.Views;

public partial class SettingsWindow : Window
{
    private LocalizationService? _localization;
    private ScrollViewer? _contentScrollViewer;
    private bool _isScrollingToCategory = false; // 防止循环更新
    private readonly Dictionary<string, Avalonia.Media.IBrush?> _originalBackgrounds = new();
    
    public SettingsWindow()
    {
        InitializeComponent();
        
        // 订阅语言变化事件
        _localization = LocalizationService.Instance;
        _localization.PropertyChanged += Localization_PropertyChanged;
    }

    protected override void OnLoaded(Avalonia.Interactivity.RoutedEventArgs e)
    {
        base.OnLoaded(e);
        
        if (DataContext is SettingsViewModel vm)
        {
            vm.TopLevel = this;
            vm.PropertyChanged += ViewModel_PropertyChanged;
            
            // 应用初始毛玻璃设置
            ApplyAcrylicEffect(vm.EnableAcrylic);
        }
        
        // 获取滚动视图引用
        _contentScrollViewer = this.FindControl<ScrollViewer>("ContentScrollViewer");
        
        // 监听滚动事件
        if (_contentScrollViewer != null)
        {
            _contentScrollViewer.ScrollChanged += ContentScrollViewer_ScrollChanged;
            
            // 初始化时检测一次当前区域
            Dispatcher.UIThread.Post(() =>
            {
                UpdateActiveCategory();
            }, DispatcherPriority.Loaded);
        }
        
        // 更新窗口标题
        UpdateWindowTitle();
    }
    
    private void MainContainer_Loaded(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (sender is Border border)
        {
            // 设置 BoxShadow
            var boxShadow = new Avalonia.Media.BoxShadow
            {
                Blur = 20,
                Spread = 0,
                OffsetX = 0,
                OffsetY = 8,
                Color = Avalonia.Media.Color.FromArgb(64, 0, 0, 0) // #40000000
            };
            border.BoxShadow = new Avalonia.Media.BoxShadows(boxShadow);
        }
    }
    
    private void ApplyAcrylicEffect(bool enable)
    {
        TransparencyLevelHint = enable 
            ? new[] { WindowTransparencyLevel.AcrylicBlur, WindowTransparencyLevel.Blur, WindowTransparencyLevel.Transparent }
            : new[] { WindowTransparencyLevel.None };
        
        // 窗口背景始终透明以支持毛玻璃效果
        Background = Avalonia.Media.Brushes.Transparent;

        var isDark = (DataContext as SettingsViewModel)?.IsDarkMode ?? (Models.AppSettings.Load().ThemeMode == "Dark");
        var acrylicBrush = CreateAcrylicSurfaceBrush(isDark);

        // 关键区域背景改为半透明，否则会被不透明卡片背景“遮住模糊”
        SetBorderBackground("MainContainer", enable, acrylicBrush);
        SetBorderBackground("TitleBar", enable, acrylicBrush);
        SetBorderBackground("NavPane", enable, acrylicBrush);
        SetBorderBackground("BottomBar", enable, acrylicBrush);
        
        // 同时应用到主窗口（如果存在）
        ApplyAcrylicToMainWindow(enable);
    }
    
    private Avalonia.Media.IBrush CreateAcrylicSurfaceBrush(bool isDark)
    {
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
    
    private void ApplyAcrylicToMainWindow(bool enable)
    {
        // 查找主窗口并应用毛玻璃效果
        var mainWindow = Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop
            ? desktop.Windows.OfType<Views.MainWindow>().FirstOrDefault()
            : null;
            
        if (mainWindow != null)
        {
            // 调用主窗口的公共方法来更新毛玻璃效果
            mainWindow.ApplyAcrylicEffect();
        }
    }
    
    private void ContentScrollViewer_ScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        // 如果正在通过导航跳转，不更新导航选中项
        if (_isScrollingToCategory)
            return;
        
        // 延迟执行，确保布局已完成
        Dispatcher.UIThread.Post(() =>
        {
            UpdateActiveCategory();
        }, DispatcherPriority.Loaded);
    }
    
    private void UpdateActiveCategory()
    {
        if (_contentScrollViewer == null || DataContext is not SettingsViewModel vm)
            return;
        
        var scrollOffset = _contentScrollViewer.Offset.Y;
        var viewportHeight = _contentScrollViewer.Viewport.Height;
        
        if (viewportHeight <= 0)
            return;
        
        var viewportTop = scrollOffset;
        var viewportCenter = scrollOffset + viewportHeight / 2;
        
        // 检查各个区域的位置
        var sections = new[]
        {
            ("Appearance", this.FindControl<StackPanel>("AppearanceSection")),
            ("Storage", this.FindControl<StackPanel>("StorageSection")),
            ("Behavior", this.FindControl<StackPanel>("BehaviorSection")),
            ("Cache", this.FindControl<StackPanel>("CacheSection")),
            ("Data", this.FindControl<StackPanel>("DataSection"))
        };
        
        string? activeCategory = null;
        double minDistance = double.MaxValue;
        
        foreach (var (categoryName, section) in sections)
        {
            if (section == null)
                continue;
            
            // 获取相对于 ScrollViewer 的位置
            var visual = section as Visual;
            if (visual == null)
                continue;
                
            var transform = visual.TransformToVisual(_contentScrollViewer);
            if (transform == null)
                continue;
            
            var sectionTop = transform.Value.Transform(new Avalonia.Point(0, 0)).Y;
            var sectionBottom = sectionTop + section.Bounds.Height;
            
            // 如果视口中心在区域内，优先选择
            if (viewportCenter >= sectionTop && viewportCenter <= sectionBottom)
            {
                activeCategory = categoryName;
                break;
            }
            
            // 计算距离视口中心的距离
            var sectionCenter = sectionTop + section.Bounds.Height / 2;
            var distance = Math.Abs(sectionCenter - viewportCenter);
            
            if (distance < minDistance)
            {
                minDistance = distance;
                activeCategory = categoryName;
            }
        }
        
        // 更新选中的类别（需要将键转换为本地化的名称）
        if (activeCategory != null)
        {
            var categoryKeys = vm.SettingsCategoryKeys;
            var categoryNames = vm.SettingsCategories;
            var categoryIndex = categoryKeys.IndexOf(activeCategory);
            var localizedCategory = categoryIndex >= 0 && categoryIndex < categoryNames.Count 
                ? categoryNames[categoryIndex] 
                : activeCategory;
            
            if (vm.SelectedCategory != localizedCategory)
            {
                _isScrollingToCategory = true;
                vm.SelectedCategory = localizedCategory;
                Dispatcher.UIThread.Post(() =>
                {
                    _isScrollingToCategory = false;
                }, DispatcherPriority.Loaded);
            }
        }
    }
    
    private void ViewModel_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(SettingsViewModel.SelectedCategory) && _contentScrollViewer != null)
        {
            // 延迟执行，确保UI已更新
            _isScrollingToCategory = true;
            Dispatcher.UIThread.Post(() =>
            {
                ScrollToCategory();
                // 延迟重置标志，确保滚动完成
                Dispatcher.UIThread.Post(() =>
                {
                    _isScrollingToCategory = false;
                }, DispatcherPriority.Loaded);
            }, DispatcherPriority.Loaded);
        }
    }
    
    private void ScrollToCategory()
    {
        if (DataContext is not SettingsViewModel vm || _contentScrollViewer == null)
            return;
        
        var category = vm.SelectedCategory;
        Control? targetSection = null;
        
        // 根据选中的类别找到对应的区域
        // 需要将本地化的分类名称转换为键
        var categoryKeys = vm.SettingsCategoryKeys;
        var categoryNames = vm.SettingsCategories;
        var categoryIndex = categoryNames.IndexOf(category);
        var categoryKey = categoryIndex >= 0 && categoryIndex < categoryKeys.Count 
            ? categoryKeys[categoryIndex] 
            : (categoryKeys.Contains(category) ? category : "Appearance"); // 如果找不到，尝试作为键，否则默认外观
        
        targetSection = categoryKey switch
        {
            "Appearance" => this.FindControl<StackPanel>("AppearanceSection"),
            "Storage" => this.FindControl<StackPanel>("StorageSection"),
            "Behavior" => this.FindControl<StackPanel>("BehaviorSection"),
            "Cache" => this.FindControl<StackPanel>("CacheSection"),
            "Data" => this.FindControl<StackPanel>("DataSection"),
            _ => null
        };
        
        if (targetSection != null)
        {
            // 滚动到目标区域，添加一些偏移量使标题可见
            targetSection.BringIntoView();
        }
    }
    
    private void Localization_PropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        // 当语言变化时，更新窗口标题和所有绑定
        if (e.PropertyName == "CurrentLanguage" || e.PropertyName?.StartsWith("Item[") == true)
        {
            UpdateWindowTitle();
        }
    }
    
    private void UpdateWindowTitle()
    {
        if (_localization != null)
        {
            Title = _localization["SettingsTitle"];
        }
    }
    
    private void CloseButton_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        Close();
    }
    
    private void TitleBar_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }
    
    protected override void OnClosed(EventArgs e)
    {
        // 取消订阅事件
        if (_localization != null)
        {
            _localization.PropertyChanged -= Localization_PropertyChanged;
        }
        
        if (DataContext is SettingsViewModel vm)
        {
            vm.PropertyChanged -= ViewModel_PropertyChanged;
        }
        
        if (_contentScrollViewer != null)
        {
            _contentScrollViewer.ScrollChanged -= ContentScrollViewer_ScrollChanged;
        }
        
        base.OnClosed(e);
    }
}
