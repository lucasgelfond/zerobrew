<div align="center">

<h2>zerobrew</h2>

<p align="center">
  <a href="README.md">English</a> ·
  <strong>中文</strong>
</p>

[![Lint](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/ci.yml)
[![Test](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml/badge.svg)](https://github.com/lucasgelfond/zerobrew/actions/workflows/test.yml)
[![Release](https://img.shields.io/github/v/release/lucasgelfond/zerobrew?display_name=tag)](https://github.com/lucasgelfond/zerobrew/releases)
[![Discord](https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white)](https://discord.gg/ZaPYwm9zaw)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE-MIT.md)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](./LICENSE-APACHE.md)

<img alt="zerobrew demo" src="./assets/zb-demo.gif" />

<p><strong>zerobrew 为 macOS 和 Linux 上的 Homebrew 软件包带来了类似 uv 的架构。</strong></p>

</div>

## 安装 (Install)

```bash
curl -fsSL https://zerobrew.rs/install | bash
```

或通过 Homebrew 安装：

```bash
brew tap lucasgelfond/zerobrew && brew install zerobrew
```

安装完成后，运行它打印的 `export` 命令（或重启终端）。

## 快速开始 (Quick start)

```bash
zb install jq                   # 安装单个软件包
zb install wget git             # 安装多个软件包
zb bundle                       # 从 Brewfile 安装
zb bundle install -f myfile     # 从自定义文件安装
zb bundle dump                  # 将已安装的软件包导出到 Brewfile
zb bundle dump -f out --force   # 导出到自定义文件（覆盖）
zb uninstall jq                 # 卸载单个软件包
zb reset                        # 卸载所有内容
zb gc                           # 垃圾回收未使用的存储条目
zbx jq --version                # 在不链接的情况下运行
```

## 性能快照 (Performance snapshot)

<div align="center">

| 软件包 | Homebrew | ZB (冷启动) | ZB (热启动) | 冷启动加速 | 热启动加速 |
|---------|----------|-----------|-----------|--------------|--------------|
| **总体 (前 100 名)** | 452s | 226s | 59s | **2.0x** | **7.6x** |
| ffmpeg | 3034ms | 3481ms | 688ms | 0.9x | 4.4x |
| libsodium | 2353ms | 392ms | 130ms | 6.0x | 18.1x |
| sqlite | 2876ms | 625ms | 159ms | 4.6x | 18.1x |
| tesseract | 18950ms | 5536ms | 643ms | 3.4x | 29.5x |

</div>

## 与 Homebrew 的关系 (Relationship with Homebrew)

zerobrew 更像是一个针对 Homebrew 生态系统进行性能优化的客户端。我们依赖于：
- Homebrew 的 formula 定义 (homebrew-core)
- Homebrew 提供的预构建 bottle（如果可用）
- Homebrew 的软件包元数据和基础设施

我们的创新重点在于：
- 用于去重的基于内容的寻址存储 (Content-addressable storage)
- 用于零开销复制的 APFS clonefiles
- 使用 Homebrew 的 Ruby DSL 的源码编译回退 (Source build fallback)

zerobrew 处于实验阶段。我们建议将其与 Homebrew 并行运行，而不是作为替代品。除非您完全确定其影响，否则**不**建议清除 Homebrew 并将其替换为 zerobrew。

## 项目状态 (Project status)

<div align="center">
  <a href="https://star-history.com/#lucasgelfond/zerobrew&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=lucasgelfond/zerobrew&type=Date&theme=dark" />
      <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=lucasgelfond/zerobrew&type=Date" />
    </picture>
  </a>
</div>

- **状态：** 处于实验阶段，但对于许多常见的 Homebrew formulas 已经非常有用。
- **反馈：** 如果遇到不兼容问题，请提出 issue 或 PR。
- **许可证：** 根据您的选择，在 [Apache 2.0](./LICENSE-APACHE.md) 或 [MIT](./LICENSE-MIT.md) 下获得双重许可。
