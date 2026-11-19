@echo off
chcp 65001 >nul
echo ========================================
echo ShibaPack 单文件发布脚本
echo ========================================
echo.

echo [1/3] 清理之前的发布文件...
if exist "bin\Release\net8.0\win-x64\publish" (
    rmdir /s /q "bin\Release\net8.0\win-x64\publish"
)
if exist "bin\Release\net8.0\win-x64" (
    rmdir /s /q "bin\Release\net8.0\win-x64"
)
echo 清理完成
echo.

echo [2/3] 开始发布单文件可执行程序（框架依赖模式）...
dotnet publish -c Release --self-contained false -p:PublishSingleFile=true -p:PublishReadyToRun=true
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo ❌ 发布失败！
    pause
    exit /b 1
)
echo.

echo [3/3] 发布完成！
echo.
echo 📦 单文件位置：
echo    bin\Release\net8.0\publish\ShibaPack.exe
echo.
echo 💡 提示：
echo    - 这是一个单文件可执行程序（框架依赖模式）
echo    - 文件大小约 5-15MB（不包含 .NET 运行时）
echo    - 需要目标机器安装 .NET 8.0 Desktop Runtime
echo    - 下载地址：https://dotnet.microsoft.com/download/dotnet/8.0
echo.
pause

