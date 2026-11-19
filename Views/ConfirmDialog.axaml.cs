using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;

namespace ShibaPack.Views;

public partial class ConfirmDialog : Window
{
    public bool IsConfirmed { get; private set; }

    public ConfirmDialog()
    {
        InitializeComponent();
    }

    public ConfirmDialog(string title, string message)
    {
        InitializeComponent();
        
        Title = title;
        
        var messageText = this.FindControl<TextBlock>("MessageText");
        if (messageText != null) messageText.Text = message;
    }

    private void OnConfirmClick(object? sender, RoutedEventArgs e)
    {
        IsConfirmed = true;
        Close();
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e)
    {
        IsConfirmed = false;
        Close();
    }
}

