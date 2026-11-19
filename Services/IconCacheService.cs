using System;
using System.Collections.Concurrent;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using AvaloniaImage = Avalonia.Media.Imaging.Bitmap;
using SystemBitmap = System.Drawing.Bitmap;
using SystemIcon = System.Drawing.Icon;
using System.Drawing;
using System.Drawing.Imaging;

namespace ShibaPack.Services;

/// <summary>
/// 图标提取和缓存服务
/// </summary>
public class IconCacheService
{
    private readonly ConcurrentDictionary<string, AvaloniaImage?> _iconCache = new();
    private string _cacheDirectory;

    /// <summary>
    /// 创建图标缓存服务（默认使用已安装程序图标缓存目录）
    /// </summary>
    public IconCacheService() : this(DataDirectoryService.InstalledIconCacheDirectory)
    {
    }
    
    /// <summary>
    /// 创建图标缓存服务（指定缓存目录）
    /// </summary>
    /// <param name="cacheDirectory">缓存目录路径</param>
    public IconCacheService(string cacheDirectory)
    {
        _cacheDirectory = cacheDirectory;
        EnsureCacheDirectoryExists();
    }
    
    private void EnsureCacheDirectoryExists()
    {
        try
        {
            if (!Directory.Exists(_cacheDirectory))
            {
                Directory.CreateDirectory(_cacheDirectory);
            }
        }
        catch
        {
            // 静默处理
        }
    }

    /// <summary>
    /// 更新缓存目录（当安装包目录变化时调用）
    /// </summary>
    public void UpdateCacheDirectory(string? cacheDirectory = null)
    {
        _cacheDirectory = cacheDirectory ?? DataDirectoryService.InstalledIconCacheDirectory;
        EnsureCacheDirectoryExists();
    }

    /// <summary>
    /// 获取文件图标（带缓存）
    /// </summary>
    public async Task<AvaloniaImage?> GetFileIconAsync(string filePath)
    {
        if (string.IsNullOrEmpty(filePath) || !File.Exists(filePath))
            return null;

        // 检查内存缓存
        if (_iconCache.TryGetValue(filePath, out var cachedIcon))
            return cachedIcon;

        // 生成缓存文件名
        var fileName = Path.GetFileName(filePath);
        var cacheFileName = $"{GetFileHash(filePath)}.png";
        var cachePath = Path.Combine(_cacheDirectory, cacheFileName);

        // 检查磁盘缓存
        if (File.Exists(cachePath))
        {
            try
            {
                var bitmap = new AvaloniaImage(cachePath);
                _iconCache[filePath] = bitmap;
                return bitmap;
            }
            catch
            {
                // 缓存文件损坏，删除重新提取
                File.Delete(cachePath);
            }
        }

        // 提取图标并缓存
        return await Task.Run(() =>
        {
            try
            {
                var icon = ExtractIcon(filePath);
                if (icon != null)
                {
                    // 保存到磁盘缓存
                    icon.Save(cachePath);
                    _iconCache[filePath] = icon;
                }
                return icon;
            }
            catch
            {
                return null;
            }
        });
    }

    /// <summary>
    /// 提取文件图标（Windows）
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private AvaloniaImage? ExtractIconWindows(string filePath)
    {
        try
        {
            // 优先使用 Shell API 提取高质量图标
            var icon = ExtractHighQualityIcon(filePath);
            if (icon != null)
            {
                // 转换为 Avalonia Bitmap
                using var memoryStream = new MemoryStream();
                icon.Save(memoryStream, ImageFormat.Png);
                memoryStream.Seek(0, SeekOrigin.Begin);
                
                var bitmap = new AvaloniaImage(memoryStream);
                icon.Dispose();
                return bitmap;
            }
            
            // 备用方法：使用 ExtractAssociatedIcon
            var fallbackIcon = SystemIcon.ExtractAssociatedIcon(filePath);
            if (fallbackIcon == null) return null;

            using var fallbackStream = new MemoryStream();
            fallbackIcon.ToBitmap().Save(fallbackStream, ImageFormat.Png);
            fallbackStream.Seek(0, SeekOrigin.Begin);
            
            return new AvaloniaImage(fallbackStream);
        }
        catch
        {
            return null;
        }
    }
    
    /// <summary>
    /// 使用 Shell API 提取高质量图标
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private SystemBitmap? ExtractHighQualityIcon(string filePath)
    {
        try
        {
            // 使用 SHGetFileInfo 获取大图标 (256x256)
            SHFILEINFO shinfo = new SHFILEINFO();
            const uint SHGFI_ICON = 0x100;
            const uint SHGFI_LARGEICON = 0x0;  // 大图标 (32x32)
            const uint SHGFI_SYSICONINDEX = 0x4000;
            
            // 获取系统图标索引
            IntPtr hImgSmall = SHGetFileInfo(filePath, 0, ref shinfo, 
                (uint)Marshal.SizeOf(shinfo), 
                SHGFI_ICON | SHGFI_LARGEICON | SHGFI_SYSICONINDEX);
            
            if (hImgSmall != IntPtr.Zero)
            {
                // 获取系统图标列表
                IImageList? imageList = null;
                Guid iidImageList = new Guid("46EB5926-582E-4017-9FDF-E8998DAA0950");
                
                int result = SHGetImageList(SHIL_EXTRALARGE, ref iidImageList, out imageList);
                
                if (result == 0 && imageList != null)
                {
                    IntPtr hIcon = IntPtr.Zero;
                    imageList.GetIcon(shinfo.iIcon, ILD_TRANSPARENT, ref hIcon);
                    
                    if (hIcon != IntPtr.Zero)
                    {
                        using var icon = SystemIcon.FromHandle(hIcon);
                        var bitmap = icon.ToBitmap();
                        DestroyIcon(hIcon);
                        return bitmap;
                    }
                }
                
                // 备用：使用普通大图标
                if (shinfo.hIcon != IntPtr.Zero)
                {
                    using var icon = SystemIcon.FromHandle(shinfo.hIcon);
                    var bitmap = icon.ToBitmap();
                    DestroyIcon(shinfo.hIcon);
                    return bitmap;
                }
            }
            
            return null;
        }
        catch
        {
            return null;
        }
    }
    
    // Windows API 定义
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Auto)]
    private struct SHFILEINFO
    {
        public IntPtr hIcon;
        public int iIcon;
        public uint dwAttributes;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)]
        public string szDisplayName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)]
        public string szTypeName;
    }
    
    [DllImport("shell32.dll", CharSet = CharSet.Auto)]
    private static extern IntPtr SHGetFileInfo(string pszPath, uint dwFileAttributes, 
        ref SHFILEINFO psfi, uint cbFileInfo, uint uFlags);
    
    [DllImport("shell32.dll", EntryPoint = "#727")]
    private static extern int SHGetImageList(int iImageList, ref Guid riid, out IImageList? ppv);
    
    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool DestroyIcon(IntPtr hIcon);
    
    private const int SHIL_EXTRALARGE = 0x2;  // 256x256
    private const int ILD_TRANSPARENT = 0x1;
    
    [ComImport]
    [Guid("46EB5926-582E-4017-9FDF-E8998DAA0950")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IImageList
    {
        [PreserveSig]
        int Add(IntPtr hbmImage, IntPtr hbmMask, ref int pi);
        
        [PreserveSig]
        int ReplaceIcon(int i, IntPtr hicon, ref int pi);
        
        [PreserveSig]
        int SetOverlayImage(int iImage, int iOverlay);
        
        [PreserveSig]
        int Replace(int i, IntPtr hbmImage, IntPtr hbmMask);
        
        [PreserveSig]
        int AddMasked(IntPtr hbmImage, int crMask, ref int pi);
        
        [PreserveSig]
        int Draw(ref IMAGELISTDRAWPARAMS pimldp);
        
        [PreserveSig]
        int Remove(int i);
        
        [PreserveSig]
        int GetIcon(int i, int flags, ref IntPtr picon);
    }
    
    [StructLayout(LayoutKind.Sequential)]
    private struct IMAGELISTDRAWPARAMS
    {
        public int cbSize;
        public IntPtr himl;
        public int i;
        public IntPtr hdcDst;
        public int x;
        public int y;
        public int cx;
        public int cy;
        public int xBitmap;
        public int yBitmap;
        public int rgbBk;
        public int rgbFg;
        public int fStyle;
        public int dwRop;
        public int fState;
        public int Frame;
        public int crEffect;
    }

    /// <summary>
    /// 提取文件图标
    /// </summary>
    private AvaloniaImage? ExtractIcon(string filePath)
    {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            // 非 Windows 平台返回 null
            return null;
        }

#pragma warning disable CA1416 // 验证平台兼容性
        // Windows 平台提取图标
        return ExtractIconWindows(filePath);
#pragma warning restore CA1416
    }

    /// <summary>
    /// 创建默认图标（依赖Windows API）
    /// </summary>
    [System.Runtime.Versioning.SupportedOSPlatform("windows6.1")]
    private AvaloniaImage? CreateDefaultIconWindows()
    {
        try
        {
            // 创建一个简单的默认图标 (32x32 灰色方块)
            using var bitmap = new SystemBitmap(32, 32);
            using var graphics = System.Drawing.Graphics.FromImage(bitmap);
            graphics.Clear(System.Drawing.Color.Gray);

            using var memoryStream = new MemoryStream();
            bitmap.Save(memoryStream, System.Drawing.Imaging.ImageFormat.Png);
            memoryStream.Seek(0, SeekOrigin.Begin);

            return new AvaloniaImage(memoryStream);
        }
        catch
        {
            return null;
        }
    }

    /// <summary>
    /// 创建默认图标
    /// </summary>
    private AvaloniaImage? CreateDefaultIcon()
    {
        if (!RuntimeInformation.IsOSPlatform(OSPlatform.Windows))
        {
            return null;
        }
#pragma warning disable CA1416 // 验证平台兼容性
        return CreateDefaultIconWindows();
#pragma warning restore CA1416
    }

    /// <summary>
    /// 生成文件哈希（用于缓存文件名）
    /// </summary>
    private string GetFileHash(string filePath)
    {
        var fileName = Path.GetFileName(filePath);
        var fileInfo = new FileInfo(filePath);
        var uniqueKey = $"{fileName}_{fileInfo.Length}_{fileInfo.LastWriteTime.Ticks}";
        
        using var md5 = System.Security.Cryptography.MD5.Create();
        var hash = md5.ComputeHash(System.Text.Encoding.UTF8.GetBytes(uniqueKey));
        return BitConverter.ToString(hash).Replace("-", "").ToLower();
    }

    /// <summary>
    /// 清理缓存
    /// </summary>
    public void ClearCache()
    {
        _iconCache.Clear();
        
        try
        {
            if (Directory.Exists(_cacheDirectory))
            {
                Directory.Delete(_cacheDirectory, true);
                EnsureCacheDirectoryExists();
            }
        }
        catch
        {
            // 清理失败时静默处理
        }
    }
}
