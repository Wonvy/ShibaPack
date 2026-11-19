# ShibaPack

[English](README.md) · **简体中文**

跨平台的软件安装包管理器。把本机或 NAS 上的 `.exe`、`.dmg`、`.msi`、`.deb`、`.apk` 等安装包按“**软件 → 版本 → 文件**”整理起来，看得到这台电脑装了什么，还能从在线源检查和下载新版本。

所有数据都放在你指定的仓库文件夹里，索引是根目录下一个普通的 `software.json` 文件，不使用数据库。

![软件库：按分类分组的软件，带版本、平台和大小，右边是更新概况](docs/screenshots/zh-library.png)

## 功能

- **按软件和版本整理。** 把安装包拖进来，ShibaPack 从文件名认出软件名、版本号、平台和架构，按“软件名/版本号/文件”放好。用 SHA-256 判断重复。
- **两种存放方式。** “托管”把安装包复制进仓库；“索引”只记录文件原来的位置（比如在 NAS 上），不动原文件。
- **一个软件多个版本。** 可以指定默认版本或始终用最新版本，看得出本机装的是哪个版本，旧版本可以一键清理。
- **分类、收藏和智能分类。** 分类可以分级，拖动就能归类；内置约 300 个常见软件的离线规则，能给软件推荐分类。
- **已安装软件。** 列出这台电脑上装的软件，和仓库里的软件对应起来，标出哪些可以用仓库里的安装包升级。安装、升级、卸载都在一处完成。
- **在线源。** 把软件关联到 Homebrew Cask、GitHub Releases 或 winget，检查新版本并下载进仓库。
- **真实图标。** 从安装包和已安装的软件里取图标；填了主页或关联了 GitHub 的软件，还能用网站图标或仓库主人的头像。取不到图标时显示灰底首字。
- **默认就安全。** 索引自动备份（保留最近 8 份），删除的文件先进回收站、30 天后清除，还能检查哪些文件丢了或大小变了。
- **离线目录。** 可以在仓库根目录生成一个 `index.html`，不装 ShibaPack、用任何浏览器也能翻看仓库。
- **自动更新。** ShibaPack 会从 GitHub Releases 检查并安装自己的新版本。
- 浅色和深色主题，中英文界面，可以导入语言包。

## 截图

**软件详情。** 简介、安装状态、各个版本和每个版本里的文件。

![Visual Studio Code 的详情：本机装的 1.95.3 可以升级，在线源有更新的版本，仓库里存了三个版本](docs/screenshots/zh-details.png)

**已安装软件。** 这台电脑上装了什么，仓库里有没有它的安装包。

![已安装软件列表，“仓库”一列标出仓库里有哪些软件的安装包](docs/screenshots/zh-installed.png)

**智能分类。** 先给出建议，确认之后才会归类。

![智能分类对话框：建议把 Telegram 和微信放进新建的“聊天社交”分类](docs/screenshots/zh-classify.png)

**从在线源添加。** 在 Homebrew、GitHub 或 winget 上搜索，下载合适的安装包。

![从在线源添加：在 Homebrew 上搜到 Obsidian 和它的安装包](docs/screenshots/zh-source.png)

## 下载

到 [Releases 页面](https://github.com/Wonvy/ShibaPack/releases/latest) 下载最新版本。

| 系统 | 文件 |
|---|---|
| macOS（Apple 芯片和 Intel） | `ShibaPack_<版本>_universal.dmg` |
| Windows | `ShibaPack_<版本>_x64-setup.exe` 或 `ShibaPack_<版本>_x64_en-US.msi` |
| Linux | `.AppImage`、`.deb` 或 `.rpm` |

安装包还没有做苹果或微软的系统级签名：

- **macOS：** 第一次打开会被系统拦下。到“系统设置 → 隐私与安全性”里点“仍要打开”。如果提示“已损坏”，运行一次 `xattr -dr com.apple.quarantine /Applications/ShibaPack.app`。
- **Windows：** SmartScreen 弹出时点“更多信息 → 仍要运行”。

之后 ShibaPack 会从 GitHub Releases 自动更新。

## 上手

1. 第一次启动时选一个文件夹当仓库（新建的空文件夹，或者已经放着安装包的文件夹都行）。
2. 添加安装包：把文件或文件夹拖到窗口上，或者用“添加 → 导入文件 / 导入文件夹”，也可以“添加 → 从在线源添加”。
3. 在“未分类”上用“智能分类”，一次把软件都归好类。

仓库文件夹的样子：

```
仓库/
├─ software.json            索引：分类、软件、版本、文件、设置
├─ index.html               可选的离线目录
├─ Visual Studio Code/
│  ├─ 1.96.2/
│  │  ├─ VSCodeUserSetup-x64-1.96.2.exe
│  │  └─ VSCode-darwin-universal.dmg
│  └─ 1.95.3/
│     └─ VSCodeUserSetup-x64-1.95.3.exe
└─ .shibapack/              图标、索引备份、回收站、缓存
```

平台、分类、收藏都只是索引里的字段，改了不会移动文件。索引万一丢了，靠“软件名/版本号”文件夹也能重建。

## 联网与隐私

ShibaPack 离线也能用，只在下面几种情况联网：

- **简介和图标**（可以在设置里关掉）：把软件名发给维基百科和 GitHub 查一段简介；没有图标的软件，会访问它自己的主页取网站图标，或取 GitHub 仓库主人的头像。
- **在线源**（关联或检查时）：Homebrew（`formulae.brew.sh`）、GitHub Releases、GitHub 上的 winget 软件清单。
- **自己的更新**：本仓库的 GitHub Releases（可以在设置里关掉）。

## 开发

需要 Node.js 20 以上和 Rust（stable）。

```bash
npm install
```

```bash
npm run app
```

`npm run app` 会启动桌面窗口，改动代码后自动刷新或重启。

只想看界面时可以不启动桌面窗口，在浏览器里用模拟数据预览：

```bash
npm run dev
```

然后打开 <http://localhost:1430>。地址后加 `?setup` 看首次启动的引导页，加 `?empty` 看空仓库，加 `?en` 换成英文的演示数据。

### 检查

```bash
npm run test:core
```

```bash
npm run typecheck
```

`test:core` 运行核心库的单元测试（索引读写、备份恢复、文件名识别、导入去重、增删改、图标提取、已安装软件、在线源解析、下载、网站图标、离线目录）。

需要联网的测试平时不跑，要跑的时候加上 `--ignored`：

```bash
cargo test -p shibapack-core -- --ignored
```

### 自检

单元测试和浏览器预览都够不着“真实窗口里前后端是否连通、图标能否显示”这一层。带着下面三个环境变量启动桌面应用，它会用一个临时仓库把这些链路走一遍，结果打印在终端，然后自己退出：

```bash
mkdir -p /tmp/shibapack-selftest/repo && echo '{"repoRoot": "/tmp/shibapack-selftest/repo"}' > /tmp/shibapack-selftest/config.json
```

```bash
SHIBAPACK_CONFIG=/tmp/shibapack-selftest/config.json SHIBAPACK_CACHE=/tmp/shibapack-selftest/cache SHIBAPACK_SELFTEST=/path/to/some-installer.exe ./target/debug/shibapack
```

需要先有 `npm run dev` 在运行，并且已经 `cargo build -p shibapack`（用发布构建出来的程序则不需要 `npm run dev`）。`SHIBAPACK_SELFTEST` 指向任意一个安装包；它旁边如果有一个 `lang.json`，会顺带测语言包导入。自检里的在线源几项需要联网。这两个目录变量也可以单独用来让应用使用另一份引导文件和缓存，不碰正式数据。

### 打包

本机打包当前系统的安装包（需要更新包的签名私钥 `~/.tauri/shibapack-updater.key`）：

```bash
npm run bundle
```

产物在 `target/release/bundle/` 下。

### 发布新版本

已安装的 ShibaPack 会从 GitHub Releases 自动更新。发布一个新版本的步骤：

1. 把 `package.json` 里的 `version` 改成新版本号（应用的版本号只认这一处）。
2. 提交，然后打一个同名的标签并推送：

   ```bash
   git tag v0.2.0 && git push origin main --tags
   ```

3. `.github/workflows/release.yml` 会在三个系统上构建，生成一个**草稿** Release，里面有安装包；设好了签名私钥时，还有带签名的更新包和 `latest.json`。
4. 在 GitHub 上检查这个草稿，点“Publish release”。之后已安装的用户启动时就会收到更新。

#### 更新包签名私钥

内置的自动更新只安装用 ShibaPack 私钥签过名的更新包（公钥在 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey`）。这和苹果、微软的系统级签名是两回事。

发布流程里这把私钥可以不设。不设时 Release 里只有安装包，没有更新包和 `latest.json`：已安装的 ShibaPack 不会自动更新到这个版本，要用户自己下载。想生成更新包，就把私钥放进仓库的 Secrets（Settings → Secrets and variables → Actions → New repository secret）：

- 名称：`TAURI_SIGNING_PRIVATE_KEY`
- 值：`~/.tauri/shibapack-updater.key` 这个文件的全部内容

在 macOS 上把私钥复制到剪贴板：

```bash
pbcopy < ~/.tauri/shibapack-updater.key
```

这把私钥没有设密码，也不在仓库里。请另外备份一份：丢了它，已安装的 ShibaPack 就再也不能自动更新了。

### 结构

```
crates/core/   核心逻辑，不依赖界面，可单独测试
src-tauri/     桌面外壳（Tauri 2）：命令、事件、窗口
src/           界面（React + TypeScript + Tailwind）
docs/PLAN.md   方案、数据格式、阶段与进度
```
