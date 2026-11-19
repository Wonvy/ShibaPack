using System;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Material.Icons;

namespace ShibaPack.ViewModels;

public partial class IconPickerViewModel : ViewModelBase
{
    [ObservableProperty]
    private MaterialIconKind? _selectedIcon;
    
    // 计算属性：是否有选中的图标
    public bool IsIconSelected => SelectedIcon != null;
    
    partial void OnSelectedIconChanged(MaterialIconKind? value)
    {
        OnPropertyChanged(nameof(IsIconSelected));
        // 确保命令可以重新评估
        ConfirmCommand?.NotifyCanExecuteChanged();
    }
    
    public RelayCommand? ConfirmCommand { get; set; }
    public RelayCommand? CancelCommand { get; set; }
}

