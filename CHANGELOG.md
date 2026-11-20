# 更新日志 / Changelog

每个版本一节，标题写成 `## 版本号`，下面分“中文”和“English”两段。
发布时 `.github/workflows/release.yml` 把这个版本的那一节放进 GitHub Release 的说明和更新用的 `latest.json`，
ShibaPack 的“软件更新”窗口（点右下角的版本号打开）显示的就是它，按界面语言挑一段。

One section per version, titled `## <version>`, with a Chinese and an English part. When a release is built,
`.github/workflows/release.yml` puts that section into the GitHub Release notes and `latest.json`; the update window
in ShibaPack (opened from the version number in the bottom-right corner) shows the part that matches the interface language.

## 0.7.0

### 中文

- 导入预览改为紧凑列表，显示安装包图标、软件名称、文件属性版本和文件名版本。可逐项编辑名称和入库版本，匹配已有软件时会提示归入对应软件。
- 新建仓库预置八个分类。启用智能分类后，导入列表逐行显示分类预览，导入完成后自动为尚未分类的软件归类。
- 设置中可启用 theSVG 补齐缺失的软件图标，下载后可离线显示。
- AI 模型设置里的 API 密钥预览最多显示末尾六位。顶部搜索框会随中间列表栏调整位置并居中。
- 修复 Windows 复制导入安装包时的权限错误。NSIS 更新安装器会在安装目录需要额外权限时请求授权。

### English

- Import preview shows a compact list with installer icons, app names, and versions from both file metadata and filenames. Edit the app name and version in each row, and review which entries will join existing apps.
- New repositories start with eight categories. Smart categorization previews categories in each import row and applies them to uncategorized apps after import.
- Enable theSVG in Settings to fill in missing app icons. Downloaded icons are available offline.
- API key previews in AI model settings show up to the last six characters. The top search box stays centered over the main list.
- Fixed a Windows permissions error when copying installers into the repository. The NSIS updater requests authorization when the installation directory requires it.

## 0.6.0

### 中文

- 安装包和已安装软件的图标可随仓库索引保存，迁移到另一台电脑时也能显示。改进 Windows 图标提取，识别 `.appinstaller`，并为 ZIP、RAR 等压缩包显示文件类型图标。
- 在软件库、已安装和启动与服务的项目上悬停，可以查看摘要和文件路径。详情栏里的版本和安装包也支持悬停查看。
- 软件库工具栏可以直接切换列表、网格和文件夹视图。网格随栏宽变化平滑重排。左侧栏默认按标题调整宽度，右侧详情栏开合时桌面窗口可以跟随调整宽度。
- 搜索框默认提供 GitHub 和 Homebrew 快捷图标，右键可以增删，长按拖动可以排序。GitHub 搜索结果显示仓库所有者的头像。
- 添加菜单移到中间栏右下角。按住鼠标中键可以移动圆形加号按钮，右键可以复位。
- 状态栏与左侧设置区对齐。点击文件夹图标可更换仓库位置，新版本可用时版本号旁显示彩色上箭头。

### English

- Installer and installed-app icons can travel with the repository index and display on another computer. Windows icon extraction is more reliable, `.appinstaller` files are recognized, and ZIP and RAR archives have file-type icons.
- Hover over items in the library, installed apps, or Startup & services to see summaries and file paths. Versions and installers in the details pane also have hover cards.
- Switch directly between list, grid, and folder views from the library toolbar. Grid items animate as the pane changes width. The sidebar fits its labels by default, and the desktop window can resize when the details pane opens or closes.
- GitHub and Homebrew shortcuts appear in the search box by default. Right-click to choose which shortcuts are shown, and press and drag to reorder them. GitHub results show repository owners' avatars.
- The Add menu has moved to a round button in the lower-right corner of the main pane. Drag it with the middle mouse button and right-click to reset its position.
- The status bar lines up with the sidebar's Settings area. Click its folder icon to switch repositories. A colored arrow appears next to the version number when an update is available.

## 0.5.0

### 中文

- 导入窗口：把文件拖进窗口后先弹出窗口，看每个文件会被认成什么，选放进当前分类、智能分类、指定分类还是不分类，再选复制、移动或只记录位置。确认后窗口不关，显示进度，导完列出结果；选了智能分类时可以接着给这次导入的软件分类
- 直接拖到左栏的某个分类上，就导入进那个分类
- 文件夹可以当作一个安装包：绿色版软件的文件夹、macOS 的 `.app` 整个导入，版本号、发布者和图标从里面的主程序取，“运行”打开主程序
- 文件名里看不出版本号时，读 exe、msi 文件属性里的产品版本、产品名和公司名（比如 WinThumbsPreloader.exe 会认成 1.1.0），读过的会缓存；以前放在“unknown”里的会在后台自动挪到对应的版本
- 导入失败时写清楚是哪个文件、在哪一步、什么原因（没有权限、磁盘满、文件被占用、网络磁盘断开……）
- 新增失败日志：导入、更新、启动项操作失败时记下来，在“设置 → 失败日志”里查看
- 网格视图里鼠标停一下就浮出软件简介
- 启动与服务的列表可以调列宽、排序、多选和全选，右键让 AI 解释；多项一起解释时合成一次请求，快很多
- 列表里不再在鼠标停上去时显示卸载按钮，卸载都在右键菜单里
- 修复：Windows 上用安装程序（setup.exe）装的 ShibaPack 自动更新时没有权限。macOS 上替换应用被系统拦下时，更新窗口会说明原因，并可以打开“App 管理”设置或下载安装包手动安装

### English

- An import window: dropping files onto the window first shows what each file will be recognized as, lets you put them in the current category, smart categorize them, pick a category or leave them uncategorized, and choose copy, move or index only. The window stays open while importing, shows progress and ends with the results; with smart categorize you can go straight on to categorizing the new apps
- Drop files on a category in the sidebar to import them straight into it
- Folders can be installers: a portable app's folder or a macOS `.app` is imported as one app, with the version, publisher and icon taken from the main program inside; Run opens that program
- When the file name has no version, the product version, product name and company are read from the properties of exe and msi files (WinThumbsPreloader.exe becomes 1.1.0, for example) and cached; files filed under "unknown" before are moved to the right version in the background
- Failed imports now say which file failed, at which step and why (permission denied, disk full, file in use, network drive disconnected…)
- A failure log: failed imports, updates and startup item changes are recorded under Settings → Failure log
- Hover over an app in a grid to see its summary
- The startup & services list has resizable, sortable columns, multi-select and select all, and an AI explanation from the right-click menu; several items are explained in one request, which is much faster
- Lists no longer show an uninstall button on hover; uninstall from the right-click menu
- Fixed: on Windows, ShibaPack installed with the setup.exe installer could not update itself for lack of permission. On macOS, when the system blocks replacing the app, the update window explains why and offers to open the App Management settings or download the installer

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
