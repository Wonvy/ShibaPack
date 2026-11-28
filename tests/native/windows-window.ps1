param(
  [ValidateSet('resize', 'visible', 'close')] [string] $Action,
  [int] $AppPid,
  [string] $Executable,
  [string] $Caption
)

$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class TestWindow {
  [DllImport("user32.dll", SetLastError = true)]
  public static extern bool SetWindowPos(IntPtr hwnd, IntPtr relativeTo, int x, int y, int width, int height, uint flags);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
}
"@

if ($Action -eq 'resize') {
  $process = Get-Process -Id $AppPid
  if ($process.Path -ne $Executable -or $process.MainWindowHandle -eq 0) {
    throw 'No window for the specified test executable'
  }
  if (![TestWindow]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 48, 60, 900, 665, 0x14)) {
    throw [ComponentModel.Win32Exception]::new([Runtime.InteropServices.Marshal]::GetLastWin32Error())
  }
  exit
}

if ($Caption -notmatch '^ShibaPack-Smoke-[a-f0-9-]+$') { throw 'Invalid test window caption' }
$fixture = Get-Process -Name fixture -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowTitle -ceq $Caption -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (!$fixture) { exit }
$handle = $fixture.MainWindowHandle
if ($Action -eq 'visible') {
  if ($handle -ne [IntPtr]::Zero -and [TestWindow]::IsWindowVisible($handle)) { 'visible' }
} else {
  if (!$fixture.CloseMainWindow() -or !$fixture.WaitForExit(5000)) {
    throw 'The test uninstaller window did not close'
  }
}
