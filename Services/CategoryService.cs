using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using ShibaPack.Models;

namespace ShibaPack.Services;

/// <summary>
/// 分类管理服务
/// </summary>
public class CategoryService
{
    public ObservableCollection<Category> Categories { get; } = [];
    public event Action? CategoriesChanged;

    public CategoryService()
    {
        InitializeCategories();
    }

    /// <summary>
    /// 初始化分类
    /// </summary>
    private void InitializeCategories()
    {
        // 添加预定义分类（按 SortOrder 顺序）
        Categories.Add(PredefinedCategories.InstalledPrograms);
        Categories.Add(PredefinedCategories.AllSoftware);
        Categories.Add(PredefinedCategories.Uncategorized);
        
        // 检查分类文件是否存在
        var categoriesFileExists = File.Exists(DataDirectoryService.CategoriesDataFile);
        
        // 加载用户自定义分类
        LoadCategories();
        
        // 只有在分类文件不存在时才添加默认分类（首次启动）
        // 如果文件存在但为空，说明用户已删除所有分类，不要重新添加
        if (!categoriesFileExists && Categories.Count == 3)
        {
            foreach (var category in PredefinedCategories.GetDefaults())
            {
                Categories.Add(category);
            }
            _ = SaveCategoriesAsync();
        }
    }

    /// <summary>
    /// 加载分类
    /// </summary>
    private void LoadCategories()
    {
        try
        {
            var filePath = DataDirectoryService.CategoriesDataFile;
            System.Diagnostics.Debug.WriteLine($"[LoadCategories] 分类文件路径: {filePath}");
            
            if (File.Exists(filePath))
            {
                var json = File.ReadAllText(filePath);
                System.Diagnostics.Debug.WriteLine($"[LoadCategories] 读取到JSON: {json.Substring(0, Math.Min(200, json.Length))}...");
                
                var categories = JsonSerializer.Deserialize<List<Category>>(json);
                
                if (categories != null)
                {
                    System.Diagnostics.Debug.WriteLine($"[LoadCategories] 反序列化得到 {categories.Count} 个分类");
                    foreach (var c in categories)
                    {
                        System.Diagnostics.Debug.WriteLine($"  - ID={c.Id}, Name={c.Name}");
                    }
                    
                    // 构建父子关系
                    var categoryDict = categories.ToDictionary(c => c.Id);
                    var rootCategories = categories.Where(c => string.IsNullOrEmpty(c.ParentId)).OrderBy(c => c.SortOrder).ToList();
                    
                    foreach (var rootCategory in rootCategories)
                    {
                        Categories.Add(rootCategory);
                        LoadChildCategories(rootCategory, categoryDict);
                    }
                }
            }
            else
            {
                System.Diagnostics.Debug.WriteLine($"[LoadCategories] 分类文件不存在");
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[LoadCategories] 加载失败: {ex.Message}");
        }
    }
    
    /// <summary>
    /// 递归加载子分类
    /// </summary>
    private void LoadChildCategories(Category parent, Dictionary<string, Category> categoryDict)
    {
        var children = categoryDict.Values
            .Where(c => c.ParentId == parent.Id)
            .OrderBy(c => c.SortOrder)
            .ToList();
            
        foreach (var child in children)
        {
            parent.Children.Add(child);
            LoadChildCategories(child, categoryDict);
        }
    }

    /// <summary>
    /// 保存分类
    /// </summary>
    public async Task SaveCategoriesAsync()
    {
        try
        {
            var filePath = DataDirectoryService.CategoriesDataFile;
            var dir = Path.GetDirectoryName(filePath);
            if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
            }

            // 只排除系统预定义分类（all、installed、uncategorized），其他分类都保存
            var systemIds = new[] { "all", "installed", "uncategorized" };
            var allCategories = new List<Category>();
            FlattenCategories(Categories.Where(c => !systemIds.Contains(c.Id)), allCategories);
            
            System.Diagnostics.Debug.WriteLine($"[SaveCategories] 准备保存 {allCategories.Count} 个分类到: {filePath}");
            foreach (var c in allCategories)
            {
                System.Diagnostics.Debug.WriteLine($"  - ID={c.Id}, Name={c.Name}");
            }
            
            var json = JsonSerializer.Serialize(allCategories, new JsonSerializerOptions 
            { 
                WriteIndented = true,
                Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping
            });
            await File.WriteAllTextAsync(filePath, json);
            
            System.Diagnostics.Debug.WriteLine($"[SaveCategories] 保存成功");
            CategoriesChanged?.Invoke();
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[SaveCategories] 保存失败: {ex.Message}");
        }
    }
    
    /// <summary>
    /// 将树形结构展开为平面列表
    /// </summary>
    private void FlattenCategories(IEnumerable<Category> categories, List<Category> result)
    {
        foreach (var category in categories)
        {
            result.Add(category);
            if (category.Children.Count > 0)
            {
                FlattenCategories(category.Children, result);
            }
        }
    }

    /// <summary>
    /// 检查分类名称是否重复
    /// </summary>
    private bool IsCategoryNameExists(string name, string? parentId, string? excludeId = null)
    {
        if (string.IsNullOrEmpty(parentId))
        {
            // 检查根分类中是否有重名（排除系统预定义分类）
            var systemIds = new[] { "all", "installed", "uncategorized" };
            return Categories.Any(c => 
                c.Name.Equals(name, StringComparison.OrdinalIgnoreCase) && 
                !systemIds.Contains(c.Id) &&
                (excludeId == null || c.Id != excludeId));
        }
        else
        {
            // 检查子分类中是否有重名
            var parent = FindCategoryById(parentId);
            if (parent != null)
            {
                return parent.Children.Any(c => 
                    c.Name.Equals(name, StringComparison.OrdinalIgnoreCase) &&
                    (excludeId == null || c.Id != excludeId));
            }
        }
        return false;
    }
    
    /// <summary>
    /// 添加分类
    /// </summary>
    public async Task<Category?> AddCategoryAsync(string name, string icon = "Package", string? parentId = null)
    {
        // 检查重名
        if (IsCategoryNameExists(name, parentId))
        {
            throw new InvalidOperationException(
                string.IsNullOrEmpty(parentId) 
                    ? "已存在同名的父级分类" 
                    : "已存在同名的子分类");
        }
        
        var category = new Category
        {
            Name = name,
            Icon = icon,
            ParentId = parentId,
            SortOrder = Categories.Count
        };
        
        if (string.IsNullOrEmpty(parentId))
        {
            // 添加为根分类
            Categories.Add(category);
        }
        else
        {
            // 添加为子分类
            var parent = FindCategoryById(parentId);
            if (parent != null)
            {
                category.SortOrder = parent.Children.Count;
                parent.Children.Add(category);
            }
            else
            {
                Categories.Add(category);
            }
        }
        
        await SaveCategoriesAsync();
        
        return category;
    }
    
    /// <summary>
    /// 递归查找分类
    /// </summary>
    public Category? FindCategoryById(string categoryId)
    {
        foreach (var category in Categories)
        {
            if (category.Id == categoryId)
                return category;
                
            var found = FindCategoryInChildren(category.Children, categoryId);
            if (found != null)
                return found;
        }
        return null;
    }
    
    private Category? FindCategoryInChildren(ObservableCollection<Category> children, string categoryId)
    {
        foreach (var child in children)
        {
            if (child.Id == categoryId)
                return child;
                
            var found = FindCategoryInChildren(child.Children, categoryId);
            if (found != null)
                return found;
        }
        return null;
    }

    /// <summary>
    /// 更新分类
    /// </summary>
    public async Task UpdateCategoryAsync(Category category)
    {
        // 递归搜索分类（包括子分类）
        var existing = FindCategoryById(category.Id);
        if (existing != null)
        {
            // 如果名称改变，检查重名
            if (existing.Name != category.Name && IsCategoryNameExists(category.Name, existing.ParentId, category.Id))
            {
                throw new InvalidOperationException(
                    string.IsNullOrEmpty(existing.ParentId) 
                        ? "已存在同名的父级分类" 
                        : "已存在同名的子分类");
            }
            
            existing.Name = category.Name;
            existing.Icon = category.Icon;
            await SaveCategoriesAsync();
            System.Diagnostics.Debug.WriteLine($"[UpdateCategory] 已更新分类: ID={category.Id}, Name={category.Name}");
        }
        else
        {
            System.Diagnostics.Debug.WriteLine($"[UpdateCategory] 未找到分类: ID={category.Id}");
        }
    }

    /// <summary>
    /// 删除分类
    /// </summary>
    public async Task<bool> DeleteCategoryAsync(string categoryId)
    {
        // 不能删除预定义分类
        if (categoryId == "all" || categoryId == "uncategorized")
            return false;
        
        var category = FindCategoryById(categoryId);
        if (category != null)
        {
            // 如果是根分类，直接从 Categories 中移除
            if (string.IsNullOrEmpty(category.ParentId))
            {
                Categories.Remove(category);
            }
            else
            {
                // 如果是子分类，从父分类的 Children 中移除
                var parent = FindCategoryById(category.ParentId);
                if (parent != null)
                {
                    parent.Children.Remove(category);
                }
            }
            
            await SaveCategoriesAsync();
            return true;
        }
        
        return false;
    }

    /// <summary>
    /// 重置为默认分类
    /// </summary>
    public async Task ResetToDefaultCategoriesAsync()
    {
        // 移除所有非系统分类
        var systemIds = new[] { "all", "installed", "uncategorized" };
        var toRemove = Categories.Where(c => !systemIds.Contains(c.Id)).ToList();
        foreach (var category in toRemove)
        {
            Categories.Remove(category);
        }
        
        // 添加默认分类
        foreach (var category in PredefinedCategories.GetDefaults())
        {
            Categories.Add(category);
        }
        
        await SaveCategoriesAsync();
        CategoriesChanged?.Invoke();
    }

    /// <summary>
    /// 根据ID获取分类
    /// </summary>
    public Category? GetCategoryById(string categoryId)
    {
        return FindCategoryById(categoryId);
    }

    /// <summary>
    /// 更新分类的软件数量
    /// </summary>
    public void UpdateCategoryCounts(IEnumerable<SoftwareGroup> softwareGroups)
    {
        var groups = softwareGroups.ToList();
        
        foreach (var category in Categories)
        {
            UpdateCategoryCount(category, groups);
        }
    }
    
    /// <summary>
    /// 保存软件分类关联
    /// </summary>
    public async Task SaveSoftwareCategoryAsync(string softwareName, string categoryId)
    {
        try
        {
            var filePath = DataDirectoryService.SoftwareCategoriesFile;
            var mappings = new Dictionary<string, string>();
            
            // 加载现有映射
            if (File.Exists(filePath))
            {
                var json = await File.ReadAllTextAsync(filePath);
                mappings = JsonSerializer.Deserialize<Dictionary<string, string>>(json) ?? new Dictionary<string, string>();
            }
            
            // 更新映射
            mappings[softwareName] = categoryId;
            
            // 保存
            var newJson = JsonSerializer.Serialize(mappings, new JsonSerializerOptions 
            { 
                WriteIndented = true,
                Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping
            });
            await File.WriteAllTextAsync(filePath, newJson);
        }
        catch
        {
            // 静默处理保存失败
        }
    }
    
    /// <summary>
    /// 加载软件分类映射
    /// </summary>
    public Dictionary<string, string> LoadSoftwareCategoryMappings()
    {
        try
        {
            var filePath = DataDirectoryService.SoftwareCategoriesFile;
            if (File.Exists(filePath))
            {
                var json = File.ReadAllText(filePath);
                return JsonSerializer.Deserialize<Dictionary<string, string>>(json) ?? new Dictionary<string, string>();
            }
        }
        catch
        {
            // 静默处理加载失败
        }
        return new Dictionary<string, string>();
    }
    
    /// <summary>
    /// 递归更新分类和子分类的数量
    /// </summary>
    private void UpdateCategoryCount(Category category, List<SoftwareGroup> groups)
    {
        if (category.Id == "all")
        {
            category.SoftwareCount = groups.Count;
        }
        else if (category.Id == "installed")
        {
            // "已安装"分类的数量由 ViewModel 手动维护，这里不更新
            // category.SoftwareCount 保持不变
        }
        else if (category.Id == "uncategorized")
        {
            category.SoftwareCount = groups.Count(g => g.CategoryId == "uncategorized");
        }
        else
        {
            category.SoftwareCount = groups.Count(g => g.CategoryId == category.Id);
        }
        
        // 更新子分类数量
        foreach (var child in category.Children)
        {
            UpdateCategoryCount(child, groups);
        }
    }
}
