using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Xml.Linq;

namespace ShibaPack.Services;

/// <summary>
/// 本地化服务
/// </summary>
public class LocalizationService : INotifyPropertyChanged
{
    private static LocalizationService? _instance;
    private Dictionary<string, string> _currentStrings;
    private string _currentLanguage = "zh-CN";

    public static LocalizationService Instance => _instance ??= new LocalizationService();

    public event PropertyChangedEventHandler? PropertyChanged;

    private LocalizationService()
    {
        // 从设置加载语言，如果没有设置则使用系统语言或默认中文
        var settings = Models.AppSettings.Load();
        _currentLanguage = settings.Language;
        LoadLanguage(_currentLanguage);
    }
    
    /// <summary>
    /// 获取语言文件路径
    /// </summary>
    private static string GetLanguageFilePath(string languageCode)
    {
        // 首先尝试从应用程序目录的 Languages 文件夹加载
        var appDirectory = AppContext.BaseDirectory;
        var languageFile = Path.Combine(appDirectory, "Languages", $"{languageCode}.xml");
        
        if (File.Exists(languageFile))
        {
            return languageFile;
        }
        
        // 如果不存在，尝试从嵌入资源加载（开发时）
        // 这里可以添加从嵌入资源加载的逻辑，但为了简化，我们直接返回路径
        // 如果文件不存在，LoadLanguage 会使用默认语言
        return languageFile;
    }

    /// <summary>
    /// 当前语言
    /// </summary>
    public string CurrentLanguage
    {
        get => _currentLanguage;
        set
        {
            if (_currentLanguage != value)
            {
                _currentLanguage = value;
                LoadLanguage(value);
                OnPropertyChanged();
                // 触发索引器更新
                PropertyChanged?.Invoke(this, new PropertyChangedEventArgs("Item[]"));
            }
        }
    }

    /// <summary>
    /// 获取本地化字符串
    /// </summary>
    public string this[string key]
    {
        get => _currentStrings.TryGetValue(key, out var value) ? value : key;
    }

    /// <summary>
    /// 获取格式化的本地化字符串
    /// </summary>
    public string GetString(string key, params object[] args)
    {
        var template = this[key];
        try
        {
            return string.Format(template, args);
        }
        catch
        {
            return template;
        }
    }

    /// <summary>
    /// 加载语言
    /// </summary>
    private void LoadLanguage(string languageCode)
    {
        try
        {
            var languageFile = GetLanguageFilePath(languageCode);
            
            if (File.Exists(languageFile))
            {
                _currentStrings = LoadLanguageFromXml(languageFile);
            }
            else
            {
                // 如果文件不存在，尝试加载默认语言（中文）
                var defaultFile = GetLanguageFilePath("zh-CN");
                if (File.Exists(defaultFile))
                {
                    _currentStrings = LoadLanguageFromXml(defaultFile);
                }
                else
                {
                    // 如果连默认文件都不存在，使用空字典（会显示 key）
                    _currentStrings = new Dictionary<string, string>();
                }
            }
        }
        catch (Exception ex)
        {
            // 加载失败时使用空字典
            System.Diagnostics.Debug.WriteLine($"[LocalizationService] Failed to load language {languageCode}: {ex.Message}");
            _currentStrings = new Dictionary<string, string>();
        }

        // 通知所有绑定更新
        NotifyAllPropertiesChanged();
    }
    
    /// <summary>
    /// 从 XML 文件加载语言资源
    /// </summary>
    private static Dictionary<string, string> LoadLanguageFromXml(string filePath)
    {
        var strings = new Dictionary<string, string>();
        
        try
        {
            var doc = XDocument.Load(filePath);
            var root = doc.Element("resources");
            
            if (root != null)
            {
                foreach (var element in root.Elements("string"))
                {
                    var key = element.Attribute("key")?.Value;
                    var value = element.Value;
                    
                    if (!string.IsNullOrEmpty(key) && !string.IsNullOrEmpty(value))
                    {
                        strings[key] = value;
                    }
                }
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[LocalizationService] Failed to parse XML file {filePath}: {ex.Message}");
        }
        
        return strings;
    }

    /// <summary>
    /// 获取支持的语言列表（动态从文件系统检测）
    /// </summary>
    public static List<LanguageOption> GetSupportedLanguages()
    {
        var languages = new List<LanguageOption>();
        var languagesDirectory = Path.Combine(AppContext.BaseDirectory, "Languages");
        
        // 预定义的语言映射（语言代码 -> 显示名称）
        var languageMap = new Dictionary<string, (string Name, string NativeName)>
        {
            ["zh-CN"] = ("Chinese", "简体中文"),
            ["en-US"] = ("English", "English"),
            ["ja-JP"] = ("Japanese", "日本語"),
            ["ko-KR"] = ("Korean", "한국어")
        };
        
        // 如果 Languages 目录存在，扫描 XML 文件
        if (Directory.Exists(languagesDirectory))
        {
            var xmlFiles = Directory.GetFiles(languagesDirectory, "*.xml");
            
            foreach (var file in xmlFiles)
            {
                var fileName = Path.GetFileNameWithoutExtension(file);
                
                // 从文件名提取语言代码（例如：zh-CN.xml -> zh-CN）
                if (languageMap.TryGetValue(fileName, out var langInfo))
                {
                    languages.Add(new LanguageOption
                    {
                        Code = fileName,
                        Name = langInfo.Name,
                        NativeName = langInfo.NativeName
                    });
                }
                else
                {
                    // 如果不在预定义列表中，使用文件名作为显示名称
                    languages.Add(new LanguageOption
                    {
                        Code = fileName,
                        Name = fileName,
                        NativeName = fileName
                    });
                }
            }
        }
        
        // 如果没有找到任何文件，返回默认列表
        if (languages.Count == 0)
        {
            return
            [
                new LanguageOption { Code = "zh-CN", Name = "Chinese", NativeName = "简体中文" },
                new LanguageOption { Code = "en-US", Name = "English", NativeName = "English" },
                new LanguageOption { Code = "ja-JP", Name = "Japanese", NativeName = "日本語" },
                new LanguageOption { Code = "ko-KR", Name = "Korean", NativeName = "한국어" }
            ];
        }
        
        // 按语言代码排序
        return languages.OrderBy(l => l.Code).ToList();
    }

    private void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }

    private void NotifyAllPropertiesChanged()
    {
        // 通知所有可能的键都已更改
        foreach (var key in _currentStrings.Keys)
        {
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs($"Item[{key}]"));
        }
    }
}

/// <summary>
/// 语言选项
/// </summary>
public class LanguageOption
{
    public string Code { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public string NativeName { get; set; } = string.Empty;
    
    public string DisplayName => $"{NativeName} ({Name})";
}
