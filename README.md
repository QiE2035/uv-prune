# uv-prune

一个用于清理 uv 缓存目录的工具。

## 介绍

uv 是一个极快的 Python 包安装器和解析器，使用 Rust 编写。它在使用过程中会产生缓存文件，这些缓存文件存储在特定目录中。uv-prune 是一个专门用于清理这些缓存文件的工具，可以有效减少磁盘空间占用。

## 功能

- 清理 uv 的 wheels-v5 目录中的无效缓存
- 清理 uv 的 archive-v0 目录中的非硬链接文件
- 快速扫描和清理，提升清理效率
- 支持 Windows 系统（目前主要针对 Windows 平台开发）

## 使用方法

直接运行程序即可清理 uv 缓存：

```bash
uv-prune
```

程序会自动定位 uv 缓存目录，默认位置为:

- Windows: `%LOCALAPPDATA%\uv\cache`

你也可以通过设置环境变量 `UV_CACHE_DIR` 来指定 uv 缓存目录：

```bash
UV_CACHE_DIR="/path/to/uv/cache" uv-prune
```

## 工作原理

1. **wheels-v5 目录清理**：

   - 扫描 wheels-v5 目录中的文件
   - 删除无效的缓存文件 (引用的 archive-v0 路径不存在)

2. **archive-v0 目录清理**：
   - 检测并删除非硬链接的文件
   - 清理空目录

## 特性

- 使用 rayon 并行处理，提升清理速度
- 使用 walkdir 遍历目录
- Windows 平台使用原生 API 检测硬链接

## 限制

- 目前仅支持 Windows 系统
- 仅支持 wheels-v5、archive-v0 目录清理

## 发展

- [ ] 添加 Linux 支持
- [ ] 移植 uv 缓存扫描逻辑