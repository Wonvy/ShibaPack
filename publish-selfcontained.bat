@echo off
chcp 65001 >nul
echo ========================================
echo ShibaPack 自包含单文件发布脚本
echo （包含 .NET 运行时，无需安装）
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

echo [2/3] 开始发布自包含单文件可执行程序...
dotnet publish -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:PublishReadyToRun=true
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
echo    bin\Release\net8.0\win-x64\publish\ShibaPack.exe
echo.
echo 💡 提示：
echo    - 这是一个独立的单文件可执行程序，可以直接运行
echo    - 文件大小约 100-150MB（包含 .NET 运行时）
echo    - 首次运行可能需要几秒钟解压
echo    - 无需目标机器安装 .NET 运行时
echo.
pause

