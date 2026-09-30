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

安装程序会更新你的 shell 配置。完成后，重启终端，或运行它打印的 `source` 命令。

或通过 Homebrew 安装：

```bash
brew install cachebag/zerobrew/zerobrew
```

## 更新 zerobrew (Update zerobrew)

如果使用独立安装脚本，重新运行：

```bash
curl -fsSL https://zerobrew.rs/install | bash
zb --version
```

如果通过 Homebrew 安装：

```bash
brew update && brew upgrade zerobrew
```

如果你是从旧的 `lucasgelfond/zerobrew` tap 安装的，它已不再更新。请切换到新的 tap：

```bash
brew uninstall zerobrew
brew untap lucasgelfond/zerobrew
brew install cachebag/zerobrew/zerobrew
```

`zb update` 只刷新软件包元数据。`zb upgrade` 升级通过 zerobrew 安装的软件包。它们都不会更新 `zb` 二进制文件本身。

## 快速开始 (Quick start)

```bash
zb install jq                   # 安装单个软件包
zb install wget git             # 安装多个软件包
zb bundle                       # 从 Brewfile 安装
zb bundle install -f myfile     # 从自定义文件安装
zb bundle dump                  # 将已安装的软件包导出到 Brewfile
zb bundle dump -f out --force   # 导出到自定义文件（覆盖）
zb uninstall jq                 # 卸载单个软件包
zb outdated                     # 列出有新版本可用的软件包
zb upgrade                      # 升级所有已过期的软件包
zb upgrade jq wget              # 升级指定的软件包
zb reset                        # 卸载所有内容
zb gc                           # 垃圾回收未使用的存储条目
zbx jq --version                # 在不链接的情况下运行
```

## 性能快照 (Performance snapshot)

<div align="center">

| 软件包 | Homebrew (冷启动) | ZB (冷启动) | 冷启动加速 | Homebrew (热启动) | ZB (热启动) | 热启动加速 |
|---------|-----------------|-----------|--------------|-----------------|-----------|--------------|
| **总体 (98 个软件包)** | 896s | 303s | **3.0x** | 778s | 175s | **4.5x** |
| libsodium | 2.87s | 1.14s | 2.5x | 1.88s | 334ms | 5.6x |
| tesseract | 41.99s | 10.56s | 4.0x | 39.73s | 8.26s | 4.8x |
| openjdk | 35.03s | 10.47s | 3.3x | 32.72s | 7.30s | 4.5x |
| llvm | 17.90s | 16.65s | 1.1x | 11.66s | 7.41s | 1.6x |

</div>

测试于 2026-09-30，使用 zerobrew 0.3.3 和 Homebrew 7.0.7，环境为 macOS 26.6.2、MacBook Pro（M3 Pro，18 GB 内存），下载带宽约 410 Mbit/s。测试开始时 Homebrew 中未安装任何软件包。

- **冷启动**：该软件包及其所有依赖均已卸载，下载缓存为空。
- **热启动**：该软件包及其所有依赖均已卸载，保留冷启动时的下载缓存。
- 按中位数计算，单个软件包冷启动快 2.7 倍，热启动快 5.7 倍。

<details>
<summary>完整结果</summary>

| 软件包 | Homebrew (冷启动) | ZB (冷启动) | 冷启动加速 | Homebrew (热启动) | ZB (热启动) | 热启动加速 |
|---------|-----------------|-----------|--------------|-----------------|-----------|--------------|
| ca-certificates | 9.13s | 1.29s | 7.07x | 8.20s | 290ms | 28.27x |
| openssl@3 | 12.33s | 2.72s | 4.53x | 11.29s | 960ms | 11.76x |
| xz | 2.88s | 1.11s | 2.60x | 1.87s | 313ms | 5.99x |
| icu4c@78 | 3.55s | 2.16s | 1.64x | 2.17s | 882ms | 2.46x |
| python@3.14 | 19.22s | 4.68s | 4.10x | 17.70s | 2.62s | 6.76x |
| awscli | 33.36s | 9.57s | 3.48x | 31.47s | 6.29s | 5.00x |
| node | 33.95s | 15.60s | 2.18x | 32.48s | 13.09s | 2.48x |
| harfbuzz | 25.89s | 7.35s | 3.52x | 24.40s | 5.24s | 4.66x |
| ncurses | 3.69s | 1.73s | 2.14x | 2.44s | 749ms | 3.26x |
| gh | 2.67s | 1.68s | 1.59x | 1.61s | 666ms | 2.42x |
| pcre2 | 3.02s | 1.19s | 2.54x | 1.91s | 465ms | 4.11x |
| libpng | 2.83s | 1.03s | 2.74x | 1.87s | 305ms | 6.12x |
| zstd | 4.63s | 1.57s | 2.95x | 3.55s | 537ms | 6.61x |
| glib | 7.56s | 3.69s | 2.05x | 6.37s | 2.04s | 3.12x |
| lz4 | 2.91s | 1.18s | 2.47x | 1.84s | 304ms | 6.07x |
| gettext | 5.32s | 2.41s | 2.21x | 3.97s | 1.57s | 2.52x |
| libngtcp2 | 12.49s | 2.61s | 4.78x | 11.30s | 1.12s | 10.10x |
| libnghttp3 | 2.84s | 1.12s | 2.53x | 1.85s | 299ms | 6.18x |
| pkgconf | 2.92s | 1.18s | 2.48x | 1.85s | 314ms | 5.90x |
| libunistring | 2.87s | 1.07s | 2.68x | 1.85s | 321ms | 5.75x |
| mpdecimal | 2.86s | 1.13s | 2.54x | 1.85s | 305ms | 6.08x |
| brotli | 2.81s | 1.09s | 2.59x | 1.86s | 312ms | 5.96x |
| jpeg-turbo | 2.96s | 1.17s | 2.54x | 1.90s | 313ms | 6.08x |
| xorgproto | 2.64s | 1.16s | 2.28x | 1.53s | 274ms | 5.58x |
| ffmpeg | 22.99s | 5.40s | 4.26x | 21.72s | 3.59s | 6.05x |
| cmake | 3.27s | 2.42s | 1.35x | 2.00s | 1.00s | 2.00x |
| libnghttp2 | 2.79s | 1.02s | 2.73x | 1.84s | 293ms | 6.29x |
| go | 5.40s | 6.38s | 0.85x | 3.66s | 3.28s | 1.11x |
| uv | 2.87s | 1.78s | 1.61x | 1.59s | 688ms | 2.32x |
| gmp | 2.93s | 1.14s | 2.58x | 1.86s | 316ms | 5.89x |
| libtiff | 8.91s | 2.53s | 3.52x | 7.79s | 1.44s | 5.40x |
| fontconfig | 10.72s | 3.22s | 3.33x | 9.49s | 1.77s | 5.35x |
| python@3.13 | 17.24s | 4.42s | 3.90x | 16.10s | 2.45s | 6.56x |
| git | 7.22s | 3.43s | 2.10x | 6.01s | 2.20s | 2.74x |
| little-cms2 | 9.68s | 2.69s | 3.60x | 8.84s | 1.57s | 5.62x |
| dav1d | 2.89s | 1.03s | 2.79x | 1.89s | 319ms | 5.92x |
| openexr | 12.59s | 3.08s | 4.09x | 11.32s | 1.91s | 5.91x |
| c-ares | 3.02s | 1.05s | 2.87x | 1.92s | 329ms | 5.83x |
| tesseract | 41.99s | 10.56s | 3.98x | 39.73s | 8.26s | 4.81x |
| p11-kit | 11.08s | 1.38s | 8.01x | 10.11s | 489ms | 20.66x |
| imagemagick | 17.70s | 5.68s | 3.12x | 16.59s | 4.38s | 3.79x |
| zlib | 2.88s | 1.13s | 2.55x | 1.83s | 342ms | 5.35x |
| libx11 | 6.63s | 2.65s | 2.50x | 5.82s | 1.62s | 3.60x |
| freetype | 3.69s | 1.24s | 2.98x | 2.69s | 432ms | 6.22x |
| protobuf | 4.57s | 4.80s | 0.95x | 3.50s | 3.24s | 1.08x |
| gnupg | 27.40s | 5.24s | 5.23x | 25.83s | 3.67s | 7.04x |
| openjph | 9.82s | 2.40s | 4.09x | 8.69s | 1.49s | 5.82x |
| libtasn1 | 2.75s | 1.06s | 2.58x | 1.88s | 389ms | 4.84x |
| ruby | 18.12s | 8.52s | 2.13x | 17.26s | 5.25s | 3.29x |
| gnutls | 18.73s | 3.87s | 4.84x | 17.39s | 2.52s | 6.89x |
| expat | 2.86s | 1.04s | 2.76x | 1.84s | 305ms | 6.03x |
| libsodium | 2.87s | 1.14s | 2.51x | 1.88s | 334ms | 5.61x |
| simdjson | 2.92s | 1.11s | 2.64x | 1.87s | 303ms | 6.16x |
| gemini-cli | 35.65s | 16.24s | 2.20x | 33.95s | 13.65s | 2.49x |
| libarchive | 6.22s | 1.65s | 3.78x | 5.16s | 802ms | 6.44x |
| pyenv | 15.62s | 3.34s | 4.67x | 14.18s | 1.70s | 8.33x |
| pixman | 2.88s | 1.07s | 2.70x | 1.87s | 303ms | 6.17x |
| curl | 20.14s | 3.57s | 5.64x | 18.91s | 2.29s | 8.24x |
| opus | 2.98s | 1.11s | 2.70x | 1.90s | 307ms | 6.18x |
| unbound | 14.78s | 2.82s | 5.25x | 13.73s | 1.36s | 10.11x |
| cairo | 22.55s | 6.24s | 3.62x | 21.26s | 4.57s | 4.65x |
| pango | 29.43s | 8.03s | 3.67x | 28.14s | 6.06s | 4.64x |
| leptonica | 11.62s | 3.19s | 3.64x | 10.58s | 2.09s | 5.06x |
| libxcb | 5.77s | 2.27s | 2.54x | 4.71s | 1.43s | 3.29x |
| jpeg-xl | 16.20s | 4.30s | 3.76x | 15.10s | 3.04s | 4.97x |
| coreutils | 3.82s | 1.80s | 2.12x | 2.77s | 864ms | 3.20x |
| certifi | 9.62s | 1.06s | 9.05x | 8.86s | 278ms | 31.87x |
| krb5 | 12.61s | 2.96s | 4.25x | 11.49s | 1.66s | 6.93x |
| docker | 2.64s | 1.34s | 1.97x | 1.53s | 492ms | 3.12x |
| libheif | 13.27s | 3.53s | 3.76x | 12.23s | 2.25s | 5.43x |
| webp | 5.36s | 1.49s | 3.60x | 4.40s | 804ms | 5.47x |
| libxext | 7.61s | 3.06s | 2.49x | 6.66s | 1.74s | 3.83x |
| libxau | 3.25s | 1.17s | 2.78x | 2.35s | 376ms | 6.26x |
| gcc | 13.09s | 6.62s | 1.98x | 10.37s | 3.33s | 3.11x |
| bzip2 | 2.56s | 1.19s | 2.15x | 1.48s | 362ms | 4.08x |
| libxdmcp | 3.20s | 1.17s | 2.73x | 2.34s | 371ms | 6.32x |
| abseil | 3.43s | 2.15s | 1.60x | 2.29s | 1.12s | 2.04x |
| xcbeautify | 2.45s | 1.17s | 2.10x | 1.49s | 294ms | 5.06x |
| libuv | 2.80s | 1.04s | 2.68x | 1.85s | 304ms | 6.10x |
| giflib | 2.83s | 1.03s | 2.73x | 1.85s | 299ms | 6.19x |
| utf8proc | 2.81s | 1.12s | 2.52x | 1.84s | 300ms | 6.12x |
| libxrender | 7.55s | 2.91s | 2.59x | 6.66s | 1.66s | 4.01x |
| m4 | 2.46s | 1.30s | 1.88x | 1.44s | 343ms | 4.19x |
| graphite2 | 2.85s | 1.08s | 2.63x | 1.84s | 307ms | 6.00x |
| openjdk | 35.03s | 10.47s | 3.34x | 32.72s | 7.30s | 4.48x |
| uvwasi | 3.66s | 1.24s | 2.95x | 2.71s | 401ms | 6.76x |
| libffi | 2.90s | 1.26s | 2.31x | 1.81s | 288ms | 6.28x |
| libdeflate | 2.92s | 1.07s | 2.71x | 1.84s | 303ms | 6.09x |
| llvm | 17.90s | 16.65s | 1.08x | 11.66s | 7.41s | 1.57x |
| aom | 4.01s | 1.48s | 2.71x | 2.76s | 621ms | 4.44x |
| lzo | 2.95s | 1.06s | 2.78x | 1.83s | 296ms | 6.20x |
| libevent | 12.33s | 2.90s | 4.25x | 11.28s | 1.06s | 10.59x |
| libgpg-error | 5.79s | 2.76s | 2.10x | 4.86s | 1.61s | 3.02x |
| libidn2 | 5.82s | 2.64s | 2.20x | 4.86s | 1.68s | 2.90x |
| berkeley-db@5 | 4.49s | 2.77s | 1.62x | 3.01s | 1.20s | 2.50x |
| deno | 12.93s | 4.67s | 2.77x | 11.62s | 3.21s | 3.62x |
| libedit | 2.93s | 1.09s | 2.69x | 1.83s | 300ms | 6.11x |
| oniguruma | 2.76s | 1.10s | 2.51x | 1.84s | 302ms | 6.10x |

</details>

如需复现，请在 Homebrew 中未安装任何软件包的机器上运行 `just bench --full results/`。该命令会重置 zerobrew，卸载其在 Homebrew 中安装的所有软件包，并将可直接用于 README 的表格写入 `results/benchmark.md`。

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
  <a href="https://star-history.dera.page/#lucasgelfond/zerobrew&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://star-history.dera.page/svg?repos=lucasgelfond/zerobrew&type=Date&theme=dark" />
      <img alt="Star History Chart" src="https://star-history.dera.page/svg?repos=lucasgelfond/zerobrew&type=Date" />
    </picture>
  </a>
</div>

- **状态：** 处于实验阶段，但对于许多常见的 Homebrew formulas 已经非常有用。
- **反馈：** 如果遇到不兼容问题，请提出 issue 或 PR。
- **许可证：** 根据您的选择，在 [Apache 2.0](./LICENSE-APACHE.md) 或 [MIT](./LICENSE-MIT.md) 下获得双重许可。
