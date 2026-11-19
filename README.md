# ShibaPack

跨平台软件安装包管理器。把本机或 NAS 上的安装包按“软件 → 版本 → 文件”整理起来，支持分类、标签、搜索、去重和一键运行。

所有数据都放在你指定的仓库文件夹里，索引是根目录的 `software.json`，不使用数据库。方案和进度见 [docs/PLAN.md](docs/PLAN.md)。

## 运行

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

然后打开 <http://localhost:1430>。地址后加 `?setup` 可以看首次启动的引导页。

## 检查

```bash
npm run test:core
```

```bash
npm run typecheck
```

`test:core` 运行核心库的单元测试（索引读写、备份恢复、文件名识别、导入去重、增删改、图标提取、已安装软件、在线源解析、下载、离线目录）。

需要联网的测试平时不跑，要跑的时候加上 `--ignored`：

```bash
cargo test -p shibapack-core -- --ignored
```

### 自检

单元测试和浏览器预览都够不着“真实窗口里前后端是否连通、图标能否显示”这一层。
带着下面三个环境变量启动桌面应用，它会用一个临时仓库把这些链路走一遍，结果打印在终端，然后自己退出：

```bash
mkdir -p /tmp/shibapack-selftest/repo && echo '{"repoRoot": "/tmp/shibapack-selftest/repo"}' > /tmp/shibapack-selftest/config.json
```

```bash
SHIBAPACK_CONFIG=/tmp/shibapack-selftest/config.json SHIBAPACK_CACHE=/tmp/shibapack-selftest/cache SHIBAPACK_SELFTEST=/path/to/some-installer.exe ./target/debug/shibapack
```

需要先有 `npm run dev` 在运行，并且已经 `cargo build -p shibapack`（用发布构建出来的程序则不需要 `npm run dev`）。`SHIBAPACK_SELFTEST` 指向任意一个安装包；它旁边如果有一个 `lang.json`，会顺带测语言包导入。自检里的在线源几项需要联网。这两个目录变量也可以单独用来让应用使用另一份引导文件和缓存，不碰正式数据。

## 打包

本机打包当前系统的安装包（需要更新包的签名私钥 `~/.tauri/shibapack-updater.key`）：

```bash
npm run bundle
```

产物在 `target/release/bundle/` 下。

安装包没有做系统级签名。macOS 上首次打开要到“系统设置 → 隐私与安全性”里放行；Windows 上 SmartScreen 弹出时点“更多信息 → 仍要运行”。

## 发布新版本

已安装的 ShibaPack 会从 GitHub Releases 自动更新。发布一个新版本的步骤：

1. 把 `package.json` 里的 `version` 改成新版本号（应用的版本号只认这一处）。
2. 提交，然后打一个同名的标签并推送：

   ```bash
   git tag v0.2.0 && git push origin main --tags
   ```

3. `.github/workflows/release.yml` 会在三个系统上构建，生成一个**草稿** Release，里面有安装包、带签名的更新包和 `latest.json`。
4. 在 GitHub 上检查这个草稿，点“发布”。之后用户那边启动时就会收到更新。

第一次发布之前，要把签名私钥放进仓库的 Secrets（Settings → Secrets and variables → Actions → New repository secret）：

- 名称：`TAURI_SIGNING_PRIVATE_KEY`
- 值：`~/.tauri/shibapack-updater.key` 这个文件的全部内容

把私钥复制到剪贴板：

```bash
pbcopy < ~/.tauri/shibapack-updater.key
```

这把私钥没有设密码，也不在仓库里。请另外备份一份：丢了它，就没法再给已安装的用户推送更新。

## 结构

```
crates/core/   核心逻辑，不依赖界面，可单独测试
src-tauri/     桌面外壳（Tauri 2）：命令、事件、窗口
src/           界面（React + TypeScript + Tailwind）
docs/PLAN.md   方案、数据格式、阶段与进度
```
