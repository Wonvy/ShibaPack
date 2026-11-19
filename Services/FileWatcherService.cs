using System;
using System.IO;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 文件监控服务
/// </summary>
public class FileWatcherService : IDisposable
{
    private FileSystemWatcher? _watcher;
    private readonly PackageParserService _parser = new();
    
    public event Action<string>? FileAdded;
    public event Action<string>? FileDeleted;
    public event Action<string, string>? FileRenamed;

    /// <summary>
    /// 开始监控目录
    /// </summary>
    public void StartWatching(string path)
    {
        StopWatching();

        if (string.IsNullOrEmpty(path) || !Directory.Exists(path))
            return;

        _watcher = new FileSystemWatcher(path)
        {
            NotifyFilter = NotifyFilters.FileName | NotifyFilters.DirectoryName | NotifyFilters.LastWrite,
            IncludeSubdirectories = true,
            EnableRaisingEvents = true
        };

        _watcher.Created += OnFileCreated;
        _watcher.Deleted += OnFileDeleted;
        _watcher.Renamed += OnFileRenamed;
    }

    /// <summary>
    /// 停止监控
    /// </summary>
    public void StopWatching()
    {
        if (_watcher != null)
        {
            _watcher.EnableRaisingEvents = false;
            _watcher.Created -= OnFileCreated;
            _watcher.Deleted -= OnFileDeleted;
            _watcher.Renamed -= OnFileRenamed;
            _watcher.Dispose();
            _watcher = null;
        }
    }

    private void OnFileCreated(object sender, FileSystemEventArgs e)
    {
        if (_parser.IsInstallerPackage(e.FullPath))
        {
            FileAdded?.Invoke(e.FullPath);
        }
    }

    private void OnFileDeleted(object sender, FileSystemEventArgs e)
    {
        if (_parser.IsInstallerPackage(e.FullPath))
        {
            FileDeleted?.Invoke(e.FullPath);
        }
    }

    private void OnFileRenamed(object sender, RenamedEventArgs e)
    {
        if (_parser.IsInstallerPackage(e.FullPath) || _parser.IsInstallerPackage(e.OldFullPath))
        {
            FileRenamed?.Invoke(e.OldFullPath, e.FullPath);
        }
    }

    public void Dispose()
    {
        StopWatching();
        GC.SuppressFinalize(this);
    }
}
