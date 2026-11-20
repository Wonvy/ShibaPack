# 更新日志 / Changelog

每个版本一节，标题写成 `## 版本号`，下面分“中文”和“English”两段。
发布时 `.github/workflows/release.yml` 把这个版本的那一节放进 GitHub Release 的说明和更新用的 `latest.json`，
ShibaPack 的“软件更新”窗口（点右下角的版本号打开）显示的就是它，按界面语言挑一段。

One section per version, titled `## <version>`, with a Chinese and an English part. When a release is built,
`.github/workflows/release.yml` puts that section into the GitHub Release notes and `latest.json`; the update window
in ShibaPack (opened from the version number in the bottom-right corner) shows the part that matches the interface language.

## 0.4.0

### 中文

- 新增“启动与服务”：列出登录时自动运行的程序和后台服务（macOS 的登录项、LaunchAgents、LaunchDaemons，Windows 的注册表启动项、“启动”文件夹和服务，Linux 的自动启动和 systemd），带系统自带的说明；可以启用、停用、启动、停止，Windows 服务可以改启动类型。设置了 AI 模型时还能让 AI 解释某一项是做什么的、能不能停用
- 顶栏搜索：右边可以选搜索范围；下拉框里同时列出软件库、已安装、启动项里的结果，还会在 GitHub 和 Homebrew 上找，点在线的结果可以查看最新版本并添加进仓库
- 已安装软件可以多选；右键可以写描述、获取简介、让 AI 写简介，列表里直接显示软件是做什么的
- 详情栏重新排版：安装状态是一张表，版本可以展开和收起，安装包一行一个
- 软件更新窗口：点右下角的版本号，查看更新说明并更新
- AI 的结果会缓存：智能分类和启动项的解释不再每次重新问 AI，点“重新分析”“重新解释”才重新问
- Windows 安装程序支持中文和英文，按系统语言自动切换；安装界面和 macOS 磁盘映像换了新的样子，安装程序带上了软件图标
- 侧栏的图标和文字对齐；筛选按钮不再高亮
- 修复：把文件拖进窗口时，不会再变成鼠标下那个软件的版本

### English

- New "Startup & services" page: lists what runs at login and in the background (login items, LaunchAgents and LaunchDaemons on macOS; Run keys, the Startup folder and services on Windows; autostart entries and systemd units on Linux) with the system's own descriptions. Enable, disable, start and stop items, and change the startup type of Windows services. With an AI model set up, the AI can explain what an item does and whether it is safe to disable
- Top-bar search: pick a scope on the right; the dropdown lists matches from the library, installed apps and startup items, and also searches GitHub and Homebrew. Click an online result to see its latest release and add it to the library
- Installed apps support multi-select; right-click to write a description, fetch a summary or have the AI write one, and the list shows what each app does
- A tidier details pane: install status in one table, collapsible versions, one line per installer
- An update window: click the version number in the bottom-right corner to read the release notes and update
- AI results are cached: smart categorize and startup explanations no longer ask the AI again each time; click "Ask again" / "Explain again" to refresh them
- The Windows installer speaks English and Chinese and follows the system language; the installer screens and the macOS disk image have a new look, and the installer carries the app icon
- Sidebar icons and labels line up; filter buttons are no longer highlighted
- Fixed: files dropped into the window no longer become a version of whatever app is under the pointer
