using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using CommunityToolkit.Mvvm.Input;
using Material.Icons;
using Material.Icons.Avalonia;
using ShibaPack.Models;
using ShibaPack.ViewModels;

namespace ShibaPack.Views;

public partial class IconPickerWindow : Window
{
    private IconPickerViewModel? _viewModel;
    private List<MaterialIconKind>? _allIcons;
    private CancellationTokenSource? _searchCancellation;
    private ObservableCollection<MaterialIconKind> _displayedIcons;
    private ScrollViewer? _scrollViewer;
    private TextBlock? _pageInfoText;
    private Button? _previousPageButton;
    private Button? _nextPageButton;
    private Button? _selectedButton; // 当前选中的按钮
    private Button? _confirmButton; // 确认按钮
    private Category? _previewCategory; // 用于预览的分类对象
    private string _originalIcon = string.Empty; // 原始图标，用于取消时还原
    private const int IconsPerPage = 32; // 每页32个图标
    private int _currentPage = 1; // 当前页码（从1开始）
    private int _totalPages = 1; // 总页数
    private bool _isSearchMode = false; // 是否为搜索模式
    private List<MaterialIconKind>? _searchResults; // 搜索结果
    
    public IconPickerWindow()
    {
        InitializeComponent();

        // 先初始化 ViewModel 和命令，再设置 DataContext（否则绑定可能在命令赋值前求值，导致按钮无效）
        _viewModel = new IconPickerViewModel();
        _viewModel.ConfirmCommand = new RelayCommand(() =>
        {
            if (_viewModel.IsIconSelected)
            {
                Close(true);
            }
        });
        _viewModel.CancelCommand = new RelayCommand(() => 
        {
            // 取消时还原原始图标
            RestoreOriginalIcon();
            Close(false);
        });
        DataContext = _viewModel;
        
        // 订阅 ViewModel 属性变化以更新按钮状态和预览
        _viewModel.PropertyChanged += (s, e) =>
        {
            if (e.PropertyName == nameof(_viewModel.IsIconSelected))
            {
                UpdateConfirmButtonState();
            }
            else if (e.PropertyName == nameof(_viewModel.SelectedIcon) && _previewCategory != null)
            {
                // 实时更新预览
                UpdatePreview();
            }
        };
        
        _displayedIcons = new ObservableCollection<MaterialIconKind>();
        IconsList.ItemsSource = _displayedIcons;
        
        // 异步加载图标，避免阻塞UI
        _ = LoadIconsAsync();
        
        // 等待UI加载完成后初始化
        Loaded += IconPickerWindow_Loaded;
        
        // 窗口关闭时还原图标（如果用户直接关闭窗口）
        Closing += (s, e) =>
        {
            if (!_viewModel?.IsIconSelected ?? true)
            {
                RestoreOriginalIcon();
            }
        };
    }
    
    /// <summary>
    /// 设置预览分类
    /// </summary>
    public void SetPreviewCategory(Category category)
    {
        _previewCategory = category;
        _originalIcon = category.Icon;
        
        // 如果有当前选中的图标，设置初始预览
        if (_viewModel?.SelectedIcon != null)
        {
            UpdatePreview();
        }
    }
    
    /// <summary>
    /// 更新预览
    /// </summary>
    private void UpdatePreview()
    {
        if (_previewCategory != null && _viewModel?.SelectedIcon != null)
        {
            _previewCategory.Icon = _viewModel.SelectedIcon.Value.ToString();
        }
    }
    
    /// <summary>
    /// 还原原始图标
    /// </summary>
    private void RestoreOriginalIcon()
    {
        if (_previewCategory != null && !string.IsNullOrEmpty(_originalIcon))
        {
            _previewCategory.Icon = _originalIcon;
        }
    }
    
    private void IconPickerWindow_Loaded(object? sender, RoutedEventArgs e)
    {
        // 查找ScrollViewer并添加滚轮事件
        _scrollViewer = this.FindControl<ScrollViewer>("ScrollViewer");
        _pageInfoText = this.FindControl<TextBlock>("PageInfoText");
        _previousPageButton = this.FindControl<Button>("PreviousPageButton");
        _nextPageButton = this.FindControl<Button>("NextPageButton");
        _confirmButton = this.FindControl<Button>("ConfirmButton");
        
        if (_scrollViewer != null)
        {
            // 监听鼠标滚轮事件
            _scrollViewer.PointerWheelChanged += ScrollViewer_PointerWheelChanged;
        }
        
        UpdatePageInfo();
        UpdateNavigationButtons();
        UpdateConfirmButtonState(); // 初始同步一次
    }
    
    private void ScrollViewer_PointerWheelChanged(object? sender, PointerWheelEventArgs e)
    {
        if (_isSearchMode || _allIcons == null)
            return;
        
        // 检测滚轮方向
        if (e.Delta.Y > 0)
        {
            // 向上滚动，上一页
            GoToPreviousPage();
        }
        else if (e.Delta.Y < 0)
        {
            // 向下滚动，下一页
            GoToNextPage();
        }
        
        e.Handled = true; // 阻止默认滚动行为
    }
    
    private async Task LoadIconsAsync()
    {
        // 在后台线程加载所有图标
        var iconTypes = await Task.Run(() =>
        {
            return Enum.GetValues<MaterialIconKind>()
                .OrderBy(x => x.ToString())
                .ToList();
        });
        
        _allIcons = iconTypes;
        _currentPage = 1;
        _totalPages = (int)Math.Ceiling((double)iconTypes.Count / IconsPerPage);
        
        // 加载第一页
        LoadPage(1);
    }
    
    private void LoadPage(int page)
    {
        if (_allIcons == null)
            return;
        
        _currentPage = Math.Max(1, Math.Min(page, _totalPages));
        
        // 计算当前页的图标范围
        var startIndex = (_currentPage - 1) * IconsPerPage;
        var pageIcons = _allIcons.Skip(startIndex).Take(IconsPerPage).ToList();
        
        // 更新显示的图标
        _displayedIcons.Clear();
        foreach (var icon in pageIcons)
        {
            _displayedIcons.Add(icon);
        }
        
        // 滚动到顶部
        if (_scrollViewer != null)
        {
            _scrollViewer.Offset = new Avalonia.Vector(_scrollViewer.Offset.X, 0);
        }
        
        UpdatePageInfo();
        UpdateNavigationButtons();
    }
    
    private void UpdateNavigationButtons()
    {
        if (_previousPageButton != null)
        {
            _previousPageButton.IsEnabled = CanGoToPreviousPage;
        }
        if (_nextPageButton != null)
        {
            _nextPageButton.IsEnabled = CanGoToNextPage;
        }
    }
    
    private void GoToNextPage()
    {
        if (_currentPage < _totalPages)
        {
            LoadPage(_currentPage + 1);
        }
    }
    
    private void GoToPreviousPage()
    {
        if (_currentPage > 1)
        {
            LoadPage(_currentPage - 1);
        }
    }
    
    public bool CanGoToPreviousPage => !_isSearchMode && _currentPage > 1;
    public bool CanGoToNextPage => !_isSearchMode && _currentPage < _totalPages;
    
    private void UpdatePageInfo()
    {
        if (_pageInfoText != null)
        {
            if (_isSearchMode && _searchResults != null)
            {
                _pageInfoText.Text = $"搜索结果: {_searchResults.Count} 个";
            }
            else
            {
                _pageInfoText.Text = $"第 {_currentPage} 页 / 共 {_totalPages} 页";
            }
        }
    }
    
    private async void SearchBox_TextChanged(object? sender, TextChangedEventArgs e)
    {
        if (sender is not TextBox searchBox || _viewModel == null)
            return;
        
        // 取消之前的搜索
        _searchCancellation?.Cancel();
        _searchCancellation = new CancellationTokenSource();
        var cancellationToken = _searchCancellation.Token;
        
        var searchText = searchBox.Text?.ToLower() ?? string.Empty;
        
        if (string.IsNullOrWhiteSpace(searchText))
        {
            // 退出搜索模式，恢复分页显示
            _isSearchMode = false;
            _searchResults = null;
            
            if (_allIcons != null)
            {
                LoadPage(1);
            }
            else
            {
                await LoadIconsAsync();
            }
            return;
        }
        
        // 延迟一下，避免用户快速输入时频繁搜索
        await Task.Delay(150, cancellationToken);
        
        if (cancellationToken.IsCancellationRequested)
            return;
        
        // 在后台线程过滤图标
        var filtered = await Task.Run(() =>
        {
            if (_allIcons == null)
            {
                _allIcons = Enum.GetValues<MaterialIconKind>()
                    .OrderBy(x => x.ToString())
                    .ToList();
            }
            
            return _allIcons
                .Where(kind => kind.ToString().ToLower().Contains(searchText))
                .ToList();
        }, cancellationToken);
        
        if (cancellationToken.IsCancellationRequested)
            return;
        
        // 搜索模式下显示所有匹配结果
        _isSearchMode = true;
        _searchResults = filtered;
        
        _displayedIcons.Clear();
        foreach (var icon in filtered)
        {
            _displayedIcons.Add(icon);
        }
        
        // 滚动到顶部
        if (_scrollViewer != null)
        {
            _scrollViewer.Offset = new Avalonia.Vector(_scrollViewer.Offset.X, 0);
        }
        
        UpdatePageInfo();
        UpdateNavigationButtons();
    }
    
    private void PreviousPageButton_Click(object? sender, RoutedEventArgs e)
    {
        GoToPreviousPage();
    }
    
    private void NextPageButton_Click(object? sender, RoutedEventArgs e)
    {
        GoToNextPage();
    }
    
    private void IconButton_Click(object? sender, RoutedEventArgs e)
    {
        if (sender is Button button && button.Tag is MaterialIconKind iconKind && _viewModel != null)
        {
            // 清除之前选中按钮的样式
            if (_selectedButton != null)
            {
                _selectedButton.Classes.Remove("selected");
            }
            
            // 设置新的选中图标
            _viewModel.SelectedIcon = iconKind;
            
            // 添加选中样式到当前按钮
            button.Classes.Add("selected");
            _selectedButton = button;
            
            // 更新文本显示
            if (SelectedIconText != null)
            {
                SelectedIconText.Text = $"已选择: {iconKind}";
            }
            
            // 更新确认按钮状态
            UpdateConfirmButtonState();
            
            // 实时更新预览（如果设置了预览分类）
            UpdatePreview();
        }
    }
    
    private void UpdateConfirmButtonState()
    {
        if (_confirmButton != null && _viewModel != null)
        {
            _confirmButton.IsEnabled = _viewModel.IsIconSelected;
        }
    }
    
    /// <summary>
    /// 设置初始选中的图标
    /// </summary>
    public void SetInitialIcon(MaterialIconKind icon)
    {
        if (_viewModel != null)
        {
            _viewModel.SelectedIcon = icon;
            UpdateConfirmButtonState();
            
            // 更新文本显示
            if (SelectedIconText != null)
            {
                SelectedIconText.Text = $"已选择: {icon}";
            }
            
            // 找到对应的按钮并高亮
            if (IconsList != null)
            {
                // 需要等待 UI 加载完成后再查找按钮
                Dispatcher.UIThread.Post(() =>
                {
                    FindAndSelectIconButton(icon);
                }, Avalonia.Threading.DispatcherPriority.Loaded);
            }
        }
    }
    
    /// <summary>
    /// 查找并选中对应的图标按钮
    /// </summary>
    private void FindAndSelectIconButton(MaterialIconKind icon)
    {
        if (IconsList == null) return;
        
        // 使用 VisualTreeHelper 查找按钮
        var buttons = IconsList.GetVisualDescendants().OfType<Button>();
        foreach (var button in buttons)
        {
            if (button.Tag is MaterialIconKind iconKind && iconKind == icon)
            {
                // 清除之前的选中
                if (_selectedButton != null)
                {
                    _selectedButton.Classes.Remove("selected");
                }
                
                // 选中当前按钮
                button.Classes.Add("selected");
                _selectedButton = button;
                break;
            }
        }
    }
    
    public MaterialIconKind? GetSelectedIcon()
    {
        return _viewModel?.SelectedIcon;
    }
}

