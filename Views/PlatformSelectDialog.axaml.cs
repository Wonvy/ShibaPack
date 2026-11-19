using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;

namespace ShibaPack.Views;

public partial class PlatformSelectDialog : Window
{
    public string SelectedPlatform { get; private set; } = string.Empty;
    public bool RememberChoice { get; private set; }
    public bool IsConfirmed { get; private set; }

    public PlatformSelectDialog()
    {
        InitializeComponent();
    }

    public PlatformSelectDialog(string fileName) : this()
    {
        var messageText = this.FindControl<TextBlock>("MessageText");
        if (messageText != null)
        {
            messageText.Text = $"文件 \"{fileName}\" 的后缀未识别，请选择文件所属的平台：";
        }
    }

    private void OnPlatformSelected(object? sender, RoutedEventArgs e)
    {
        if (sender is Button button && button.Tag is string platform)
        {
            SelectedPlatform = platform;
            
            var rememberCheckBox = this.FindControl<CheckBox>("RememberCheckBox");
            RememberChoice = rememberCheckBox?.IsChecked ?? false;
            
            IsConfirmed = true;
            Close();
        }
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e)
    {
        IsConfirmed = false;
        Close();
    }
}

