# 更新日志 / Changelog

每个版本一节，标题写成 `## 版本号`，下面分“中文”和“English”两段。
发布时 `.github/workflows/release.yml` 把这个版本的那一节放进 GitHub Release 的说明和更新用的 `latest.json`，
ShibaPack 的“软件更新”窗口（点右下角的版本号打开）显示的就是它，按界面语言挑一段。

One section per version, titled `## <version>`, with a Chinese and an English part. When a release is built,
`.github/workflows/release.yml` puts that section into the GitHub Release notes and `latest.json`; the update window
in ShibaPack (opened from the version number in the bottom-right corner) shows the part that matches the interface language.

## 0.12.0

### 中文

- 新增“监控安装”：安装前记录系统的状态，装完列出安装程序改了什么，包括程序、数据、设置、开机启动项、后台服务、驱动、快捷方式、文件关联、环境变量和注册表（Windows）。
  - 点“安装”时勾选“监控这次安装”，或者右键安装包选“监控安装”。可以选“标准”或“深度”范围。
  - 安装时屏幕右上角有一个总在最前面的小窗，实时显示改动了哪些文件，装好后可以直接在小窗里点“完成”。
  - 报告按类别分组，添加了开机启动、后台服务、驱动的会提醒；缓存、日志这类多半无关的改动默认藏起来。
  - .pkg 的安装收据、MSI 的组件清单里登记过的改动标为“已确认”；安装期间出现又被删掉的临时文件单独列出。
  - 安装程序要求重启电脑时，重启后打开 ShibaPack 会接着完成这次监控。
  - 记录存在仓库里，在软件详情的“安装记录”里可以随时打开。
- 磁盘分析新增“最大的文件”视图：当前文件夹和所有子文件夹里的文件按大小排成一个清单，显示类型、位置和修改日期，右键可以定位或移到废纸篓。
- 磁盘分析的矩形图只画一层，文件夹带图标，方块有圆角，名字居中显示；鼠标停在右边列表上，对应的色块跟着高亮，反过来也一样。
- 磁盘分析右边的列表可以拖动调整宽度，双击分隔线回到默认。
- 主页“最近安装”右上角可以在列表和图标之间切换。

### English

- New: monitor an installation. ShibaPack records the system before installing and lists what the installer changed afterwards: apps, data, settings, startup items, background services, drivers, shortcuts, file associations, environment variables and, on Windows, the registry.
  - Tick "Monitor this installation" when you click Install, or right-click an installer and choose Monitor installation. Choose a Standard or Deep scope.
  - While installing, a small always-on-top window in the top-right corner shows changed files live. Click Done right there when the installer has finished.
  - The report is grouped by category and warns about new startup items, background services and drivers. Likely unrelated changes such as caches and logs are hidden by default.
  - Changes listed in a .pkg receipt or an MSI component list are marked as confirmed. Temporary files that appeared and were deleted during installation are listed separately.
  - If the installer asks for a restart, open ShibaPack after restarting to finish monitoring.
  - Records are saved in your library; open them any time from Install records in the app details.
- Disk analysis has a new Largest files view: every file in the current folder and its subfolders, sorted by size, with type, location and modified date. Right-click to show it in the Finder or move it to the Trash.
- The disk treemap now draws a single level with folder icons, rounded blocks and centered labels. Hovering an item in the list on the right highlights its block, and the other way round.
- Drag the divider to resize the list on the right of disk analysis; double-click it to reset.
- Switch Recently installed on Home between a list and icons with the buttons in its top-right corner.

## 0.11.0

### 中文

- 侧栏可以收起成一列图标：点“主页”右边的箭头收起，收起后点同一个位置的箭头展开；鼠标停在图标上能看到名字。
- 侧栏名字更短：启动、磁盘、程序、所有、更新。
- 主页去掉了标题栏；CPU、内存的走势铺满卡片、放在数字后面；进程单独一行，软件数量和最近安装左右两栏。
- “程序”页去掉了标题和个数，侧栏里已经写了。
- 设置 → 通用里可以打开毛玻璃效果：窗口背景透出桌面并模糊（macOS 和 Windows 11）。
- 主页的系统盘卡片也改成用掉的部分铺满卡片背景。

### English

- The sidebar can collapse to a column of icons: click the arrow next to Home to collapse it, and the arrow in the same place to expand it again. Hover over an icon to see its name.
- Shorter sidebar names: Startup, Disk, Apps, All, Updates.
- Home no longer has a title bar. CPU and memory trends fill their cards behind the numbers; processes get their own row, with software counts and recently installed apps side by side below.
- The Apps page no longer repeats its title and count, which are already in the sidebar.
- Turn on frosted glass in Settings → General to let the desktop show through the window, blurred (macOS and Windows 11).
- The system disk card on Home now shows used space as a fill behind the numbers, like the CPU and memory cards.

## 0.10.0

### 中文

- 新增“磁盘分析”（侧栏“清理”下面，主页系统盘卡片上也能直接去）：选用户文件夹、一块盘或任意文件夹扫一遍，只读，几十万个文件几秒钟。
- 可以在扇形图和矩形图之间切换。点文件夹往里走，点上面的路径或扇形图中间往回走；右边列出这一层的每一项，颜色和图对得上。
- 色块右键可以进入文件夹或打开文件、在访达（资源管理器）中显示、复制路径、移到废纸篓（回收站）。系统文件夹、用户文件夹本身、应用程序和仓库不能在这里移走。
- 设置了 AI 时，鼠标停在文件夹上会显示它是做什么的、能不能删。解释单独缓存，看过的再看不用重新问。
- 大小、文件数、扫描用时和没有权限的文件夹显示在底部状态栏。
- 侧栏里“全部软件”和分类之间不再有分隔线；新建分类的按钮在“全部软件”这一行上，鼠标停上去出现。

### English

- New Disk usage page (below Clean up in the sidebar, also reachable from the system disk card on Home): scan your home folder, a disk or any folder. It's read-only and handles hundreds of thousands of files in seconds.
- Switch between a sunburst and a treemap. Click a folder to drill in; click the path above or the center of the sunburst to go back. The list on the right shows every item at the current level in matching colors.
- Right-click a block to open a folder or file, show it in Finder (File Explorer), copy its path or move it to the Trash (Recycle Bin). System folders, your home folder itself, apps and the library can't be moved from here.
- With AI set up, hovering over a folder shows what it is and whether it's safe to delete. Explanations are cached separately, so folders you've seen don't need to be asked about again.
- Size, file count, scan time and inaccessible folders are shown in the status bar at the bottom.
- The sidebar no longer has a line between All software and the categories; the new-category button appears when you hover over All software.

## 0.9.0

### 中文

- 清理列表改得更简洁：按类别分组，可以折叠，能清的、大的排在前面；底部有“全选”和“推荐”，选了多少、能腾出多少显示在状态栏。
- 扫描时显示进度，可以随时停止；结果放久了会自动重新扫描，清理完只重新量清过的那几项。清理前后对比系统报告的可用空间。
- 详情栏可以让 AI 解释某一项，右键类别标题能让 AI 一次解释一整组；还能看到这一项大约每周长多少，旁边有排除按钮。
- 可以清除 Firefox 的浏览记录：只删访问记录，书签留着。
- 清理设置里可以添加和编辑自定义规则：文件名筛选、多少天没动过、要先关掉的程序、直接删除还是移到回收站。
- 新增 Microsoft Teams 缓存。微信、企业微信、飞书、钉钉、QQ 没有官方说明哪些文件能删，所以不直接删，改为列出“××自带的清理”，一点就打开那个应用去用它自己的清理。
- 扫描时不再弹出“控制访达”的授权；需要看废纸篓里有什么时再问。停止扫描更快了。
- 卸载前先分析：列出软件本身和它的数据、快捷方式、设置、注册表项各有多大，有把握的默认勾上，拿不准的放在“需确认”。勾选后卸载和清理一次做完。
- 已安装里可以多选后一起卸载；详情栏和右键菜单有“分析”，只看占用，也可以只清理数据、不卸载。
- 新增主页（侧栏最上面，打开时默认进这里）：电脑配置，CPU、内存的实时占用和走势，系统盘可用空间，占用最多的进程，软件数量、有更新、开机自启、可清理，最近安装的软件。
- “查看全部”是任务管理器：搜索、排序全部进程，右键结束进程，退不出来时可以强制结束。系统进程不能在这里结束。
- 侧栏“已安装”移到“清理”下面；macOS 窗口左上角不再显示图标和名字。

### English

- A cleaner Clean up list: items are grouped by category and the groups can be collapsed, with cleanable and larger items first. Select all and Recommended sit at the bottom, and the selection and space to be freed show in the status bar.
- Scanning shows progress and can be stopped at any time. Results that have been around a while are rescanned automatically, and after cleaning only the cleaned items are measured again. Free space reported by the system is compared before and after.
- Ask AI to explain an item from the details pane, or a whole category from its right-click menu. The details pane also shows roughly how much an item grows per week, with an exclude button next to it.
- Firefox browsing history can be cleared: only visits are removed, bookmarks stay.
- Add and edit your own cleanup rules in the cleanup settings: name filters, days untouched, programs to quit first, and whether to delete or move to the Trash.
- Microsoft Teams caches are now included. WeChat, WeCom, Feishu/Lark, DingTalk and QQ have no official guidance on which files are safe to remove, so instead of deleting anything they get an item that opens the app to use its own cleanup.
- Scanning no longer asks for permission to control Finder; it asks only when you want to see what's in the Trash. Stopping a scan is faster.
- Uninstalling now starts with an analysis that lists the app with its data, shortcuts, settings and registry entries and their sizes. Likely items are ticked; uncertain ones wait under Check first. Uninstall and clean up in one go.
- Select several apps in Installed to uninstall them together. Analyze, in the details pane and the right-click menu, shows what an app takes up and can clean only its data without uninstalling it.
- New Home page at the top of the sidebar, opened by default: computer specs, live CPU and memory use with trends, free space on the system disk, the busiest processes, software counts, updates, login items, space to clean, and recently installed apps.
- View all opens a task manager to search and sort every process and end one from the right-click menu, with force quit if it won't close. System processes can't be ended here.
- Installed moved below Clean up in the sidebar; on macOS the window no longer shows the icon and name in the top-left corner.

## 0.8.0

### 中文

- 新增“清理”页（侧栏“启动与服务”下面），按类别列出能清理的东西、大小和说明：系统临时文件、日志和崩溃报告，Chrome、Edge、Firefox 等浏览器和 Slack、Discord、Claude 等应用的缓存，Xcode、npm、pip、Go、Cargo、Gradle、VS Code 等开发工具的缓存，Homebrew、WinGet 下载的安装包，回收站。
- 删了会自己重新生成的缓存直接删除，其余移到回收站；清空回收站、清除浏览记录这类找不回来的，确认时要再勾一次。“一键清理”只清这些安全的缓存。
- 下载文件夹和桌面上的安装包会和仓库比对：一模一样的可以放心清掉，还没收进仓库的可以直接导入。
- 可以清除浏览记录、Cookie、最近使用的文件和剪贴板，这些默认不勾。清 Cookie 时可以设保留名单，名单里的网站不用重新登录。密码、书签、自动填充一律不碰。
- 需要管理员权限的系统位置（Windows 更新下载的文件、内存转储、系统临时文件、传递优化缓存，macOS 的 /Library/Caches）一次授权清完。
- 浏览器、编辑器在运行时对应的项不能勾，可以在这里让它正常退出。系统文件夹、个人文件夹、同步盘和仓库一律不碰，删每一条之前都会再查一遍。
- 可以添加自定义清理文件夹和排除的文件夹；每次清理都有记录，详情栏能看到上次清理以后又长了多少。

### English

- New Clean up page (below Startup & services in the sidebar) lists what can be cleaned by category, with sizes and explanations: temporary files, logs and crash reports; caches of browsers such as Chrome, Edge and Firefox and of apps such as Slack, Discord and Claude; developer tool caches for Xcode, npm, pip, Go, Cargo, Gradle, VS Code and more; installers downloaded by Homebrew and WinGet; and the Trash.
- Caches that regenerate by themselves are deleted outright; everything else goes to the Trash. Items that can't be recovered, such as emptying the Trash or clearing history, ask for an extra confirmation. Clean now only cleans these safe caches.
- Installers in Downloads and on the Desktop are compared with your library: identical copies can be removed safely, and ones not yet in the library can be imported directly.
- Browsing history, cookies, recent files and the clipboard can be cleared too, but are never selected by default. A cookie keep list keeps you signed in to the sites you choose. Passwords, bookmarks and autofill are never touched.
- System locations that need administrator rights (Windows Update downloads, memory dumps, system temporary files, the Delivery Optimization cache, and /Library/Caches on macOS) are cleaned after a single authorization.
- Items belonging to a running browser or editor can't be selected until it quits, and you can ask it to quit from here. System folders, personal folders, synced folders and the library are never touched, and every item is checked again right before it is deleted.
- Add your own folders to clean or to exclude. Every cleanup is recorded, and the details pane shows how much has grown back since the last one.

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
