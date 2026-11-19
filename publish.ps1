# ShibaPack 单文件发布脚本 (PowerShell)
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "ShibaPack 单文件发布脚本" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

Write-Host "[1/3] 清理之前的发布文件..." -ForegroundColor Yellow
$publishPath = "bin\Release\net8.0\win-x64\publish"
if (Test-Path $publishPath) {
    Remove-Item -Path $publishPath -Recurse -Force
    Write-Host "清理完成" -ForegroundColor Green
} else {
    Write-Host "无需清理" -ForegroundColor Gray
}
Write-Host ""

Write-Host "[2/3] 开始发布单文件可执行程序（框架依赖模式）..." -ForegroundColor Yellow
dotnet publish -c Release --self-contained false `
    -p:PublishSingleFile=true `
    -p:PublishReadyToRun=true

if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "❌ 发布失败！" -ForegroundColor Red
    Read-Host "按 Enter 键退出"
    exit 1
}
Write-Host ""

Write-Host "[3/3] 发布完成！" -ForegroundColor Green
Write-Host ""
Write-Host "📦 单文件位置：" -ForegroundColor Cyan
Write-Host "   bin\Release\net8.0\publish\ShibaPack.exe" -ForegroundColor White
Write-Host ""
Write-Host "💡 提示：" -ForegroundColor Cyan
Write-Host "   - 这是一个单文件可执行程序（框架依赖模式）" -ForegroundColor Gray
Write-Host "   - 文件大小约 5-15MB（不包含 .NET 运行时）" -ForegroundColor Gray
Write-Host "   - 需要目标机器安装 .NET 8.0 Desktop Runtime" -ForegroundColor Yellow
Write-Host "   - 下载地址：https://dotnet.microsoft.com/download/dotnet/8.0" -ForegroundColor Cyan
Write-Host ""
Read-Host "按 Enter 键退出"

