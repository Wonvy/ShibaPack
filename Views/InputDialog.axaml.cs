using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;

namespace ShibaPack.Views;

public partial class InputDialog : Window
{
    public string InputText { get; private set; } = string.Empty;
    public bool IsConfirmed { get; private set; }
    private string _defaultValue = string.Empty;

    public InputDialog()
    {
        InitializeComponent();
    }

    public InputDialog(string title, string prompt, string defaultValue = "")
    {
        InitializeComponent();
        
        Title = title;
        _defaultValue = defaultValue;
        
        var promptText = this.FindControl<TextBlock>("PromptText");
        if (promptText != null) promptText.Text = prompt;
        
        var inputBox = this.FindControl<TextBox>("InputBox");
        if (inputBox != null)
        {
            inputBox.Text = defaultValue;
            inputBox.SelectAll();
        }
    }

    protected override void OnOpened(EventArgs e)
    {
        base.OnOpened(e);
        
        // 窗口打开后设置焦点
        Dispatcher.UIThread.Post(() =>
        {
            var inputBox = this.FindControl<TextBox>("InputBox");
            if (inputBox != null)
            {
                inputBox.Focus();
                if (!string.IsNullOrEmpty(_defaultValue))
                {
                    inputBox.SelectAll();
                }
            }
        }, DispatcherPriority.Loaded);
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            OnConfirmClick(sender, e);
        }
        else if (e.Key == Key.Escape)
        {
            OnCancelClick(sender, e);
        }
    }

    private void OnConfirmClick(object? sender, RoutedEventArgs e)
    {
        var inputBox = this.FindControl<TextBox>("InputBox");
        if (inputBox != null)
        {
            InputText = inputBox.Text ?? string.Empty;
        }
        IsConfirmed = true;
        Close();
    }

    private void OnCancelClick(object? sender, RoutedEventArgs e)
    {
        IsConfirmed = false;
        Close();
    }
}
