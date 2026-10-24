# 截图冲洗室 · Darkroom

把刚截下来的灵感挂到桌面，再把有用的部分整理成便签。一个轻巧、离线的 Windows 截图工具，使用 Tauri 2、Rust 和原生 JavaScript 构建。

## 下载与安装

前往 [Releases](https://github.com/15380193489/screenshot-darkroom/releases/latest) 下载 Windows 10 / 11 **x64** 版本：

- `Darkroom-0.2.1-windows-x64-setup.exe`：推荐安装版，安装到当前用户，提供开始菜单入口；缺少 WebView2 时自动下载运行组件。
- `Darkroom-0.2.1-windows-x64-portable.zip`：免安装版，解压后双击 `Darkroom/截图冲洗室.exe`；需要电脑已安装 Microsoft Edge WebView2 Runtime。
- `SHA256SUMS.txt`：安装包与压缩包的 SHA-256 校验值。

安装包尚未进行代码签名。软件不自动设置开机启动；每次启动 Windows 后打开一次即可常驻托盘。

## 截图 → 挂片 → 留下有用的内容

1. 启动软件，按 **Win + Shift + S** 框选截图。应用内的“截一张”也会打开 Windows 的矩形选区。
2. 完成选区、截图复制到剪贴板后，图片自动保存并挂到桌面晾片绳。**不会弹出主窗口，不会进入全屏。**
3. 拖动挂件顶部调整位置；点击照片回到冲洗台。重要图片点 **☆ 固定**，会优先显示在晾片绳上。
4. 在冲洗台框选内容，点击 **显影文字**，核对后存成摘录、链接或待办；也可以直接手写便签。
5. 在便签册搜索、编辑、复制、标为完成、回到原图，或导出 Markdown。

也支持选择图片、拖入图片，以及在非输入框区域按 **Ctrl + V** 粘贴图片。识别使用 Windows 本机 OCR，设置里可选择系统已安装的语言；没有对应 OCR 语言时可在 Windows 语言选项中安装。

## 桌面挂件与接收设置

- 挂件位于桌面层，避免遮挡正常应用；点击挂件照片可打开整理窗口。
- 挂件的 **Ⅱ / ▶** 控制自动接收，**×** 收起挂件。主窗口关闭按钮会收起到托盘；右键托盘菜单可完全退出。
- 设置可独立控制 **桌面晾片绳** 和 **截图后自动显示挂件**。关闭自动显示后仍会保存新截图；暂停自动接收则停止接收剪贴板图片。
- 展示数量支持 **3 / 4 / 6 / 8 张**，固定图片优先，随后按截图时间排列；超出数量的图片保留在收藏中。
- 自动接收只监听启动后的剪贴板图片变化，不收集文字和启动前的历史图片。复制普通图片也会接收；截图工具必须将截图复制到剪贴板。

挂件使用独立的透明窗口，不改动资源管理器的窗口归属；关闭后可重新开启，位置和开关会保存。

## 数据、备份与隐私

无需账号和 API 密钥，截图与 OCR 均在本机处理，应用不上传收藏内容。

数据位于 `%APPDATA%\com.darkroom.screenshot.desktop\`，设置中可点击 **打开数据目录**：

| 文件 | 用途 |
| --- | --- |
| `collection.json` | 原图、便签、标签及固定状态 |
| `collection.previous.json` | 上一次成功保存前的数据副本 |
| `desktop.json` | 挂件位置、数量、显示和自动挂片设置 |
| `monitor.json` | 自动接收开关 |

建议在设置里定期导出 JSON 备份。备份包含原图；导入会按 ID 合并，保留当前收藏。Markdown 和 JSON 导出写入当前用户的“下载”文件夹，每次生成新文件。

保存使用临时文件替换，并保留上一版有效数据。如果当前收藏文件损坏，程序会报错并保留原文件。需要手动恢复时，先从托盘完全退出软件，复制整个数据目录留存，再将 `collection.previous.json` 的副本改名为 `collection.json`。上一版副本不能替代独立备份。

安装版与免安装版共享当前用户的数据。发布包和仓库不含个人截图、收藏或运行缓存。安全问题与反馈注意事项见 [SECURITY.md](SECURITY.md)。

## 开发与构建

需要 Windows、Node.js 24+、Rust stable、Visual Studio C++ 构建工具、Windows SDK 及 WebView2。

```powershell
npm ci
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
npm run build
npm run package:release
```

发布文件输出到 `dist/`。`npm run dev` 启动桌面开发模式；`node scripts/preview.mjs` 可预览前端，但浏览器没有原生截图、OCR 和托盘能力。

本地原生集成验证（需要交互式 Windows 桌面）先执行 `cargo build --manifest-path src-tauri/Cargo.toml`，再执行 `npm run test:integration`。它使用独立测试资料目录与生成的图片，不操作系统剪贴板，不显示测试挂件；覆盖连续接收、自动挂片和主窗口保持隐藏。追加 `-- -DisableAutoMount` 可验证关闭自动挂片后只保存截图。

主要代码：`app/` 为冲洗台、便签和挂件；`src-tauri/src/desktop.rs` 管理桌面窗口；`snip.rs` 等待截图选区完成；`clipboard.rs` 解码剪贴板图片；`ocr.rs` 处理本机 OCR；`storage.rs` 提供保存与恢复副本。

## 发布流程

推送到 `main` 后，GitHub Actions 在 Windows 上运行测试、构建安装版、生成免安装包和校验值，并为 `package.json` 中尚未发布的版本创建 Release。已有同名 Release 会保留。版本号需同时更新 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、`src-tauri/tauri.conf.json` 和更新日志。

项目代码采用 [MIT License](LICENSE)。第三方依赖许可见 [THIRD-PARTY-LICENSES.txt](THIRD-PARTY-LICENSES.txt)；更新依赖并下载 Cargo 包后，可运行 `node scripts/prepare-licenses.mjs` 更新许可清单。参与改进与提交邮箱隐私说明见 [CONTRIBUTING.md](CONTRIBUTING.md)。本项目不宣称通用截图、OCR 或桌面便签功能的首创性。
