# MDEditor 1.0.0

轻量本地 Markdown 编辑器首个正式版 —— Rust + egui 桌面 GUI，**绿色单文件，双击即用**。

## 下载

| 文件 | 大小 | 说明 |
|------|------|------|
| **mdedit.exe** | 约 14 MB | **绿色单文件版（推荐）**，下载后双击即可运行，无需安装、无需任何依赖 |
| **MDEditor_setup_1.0.0.exe** | 约 5.7 MB | Inno Setup 安装包，含开始菜单/桌面快捷方式，并自动关联 `.md` / `.markdown` 文件 |

两个文件都是 Windows x64（兼容 Windows 10 / 11），静态编译，只依赖系统自带 DLL。

## 功能

- **极轻量**：静态编译单文件，不依赖第三方运行库，拷到任意电脑即可用
- **绿色免安装**：Windows GUI 子系统，运行时不弹控制台窗口
- **语法高亮**：标题、代码块、引用、行内代码、加粗、斜体、链接分色显示，自动加载系统中文字体（微软雅黑优先）
- **网页预览**：`Ctrl+P` 渲染成完整 HTML 并用系统默认浏览器打开，改完再按一次即刷新
- **导出 HTML**：`Ctrl+E` 另存为独立 `.html`
- **一键设为默认打开方式**：写 HKCU 注册表，免管理员权限
- **自动保存草稿**：内容自动存到同目录 `mdedit_draft.md`，下次启动自动恢复
- **纯本地**：不联网、不上传、无账号

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+N` | 新建 |
| `Ctrl+O` | 打开文件 |
| `Ctrl+S` | 保存 |
| `Ctrl+Shift+S` | 另存为 |
| `Ctrl+E` | 导出 HTML |
| `Ctrl+P` | 网页预览 |

## 使用方法

绿色版：直接双击 `mdedit.exe`，或从命令行带文件打开：

```
mdedit.exe 文档.md
```

安装包：双击 `MDEditor_setup_1.0.0.exe` 按向导安装，安装后双击任意 `.md` 文件即用 MDEditor 打开。

## 文件校验（SHA-256）

```
52d6673cc869ef394089a8c105f6dfbd608cfe04862f96706585eba983638d08  mdedit.exe
6ce9ce9023f19752eb3c516d96fbbb9dcbcfb183dfc4fa3dedd87a93c7d053fb  MDEditor_setup_1.0.0.exe
```

## 从源码构建

环境：Rust stable（Windows GNU 工具链）+ MSYS2（ucrt64 的 gcc 作链接器）。

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\msys64\ucrt64\bin;" + $env:Path
cd mdedit-gui
cargo build --release
```

产物：`mdedit-gui\target\release\mdedit.exe`

## 技术栈

`eframe` / `egui` 0.29 · `pulldown-cmark` 0.12 · `rfd` 0.14 · `winreg` 0.55 · `embed-resource` 2

---

MIT License
