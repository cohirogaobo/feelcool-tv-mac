# Feel Cool TV (macOS Direct Engine)

<p align="center">
  <strong>专为 macOS 与大屏客厅体验打造的次世代流光家庭媒体中心</strong><br>
  <em>Apple TV 级毛玻璃交互 · PSP XMB 动态流光丝带 · 原生 VideoToolbox 硬件解码 · 实时气象预警与 EPG 节目单</em>
</p>

<p align="center">
  <a href="https://github.com/cohirogaobo/feelcool-tv-mac"><img src="https://img.shields.io/badge/Platform-macOS%20(Apple%20Silicon%20%26%20Intel)-000000?style=for-the-badge&logo=apple" alt="macOS"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Engine-Tauri%202.0%20%2B%20Rust-FFC131?style=for-the-badge&logo=rust" alt="Tauri 2"></a>
  <a href="https://ffmpeg.org/"><img src="https://img.shields.io/badge/Video-VideoToolbox%20HLS-00D474?style=for-the-badge&logo=ffmpeg" alt="FFmpeg"></a>
  <a href="./showcase/index.html"><img src="https://img.shields.io/badge/3D%20Showcase-%E5%87%AF%E6%97%8B%E7%BA%AA%E4%BA%8B%20(Arc%20de%20Triomphe)-D4AF37?style=for-the-badge" alt="3D Showcase"></a>
</p>

---

## ✨ 项目简介

**Feel Cool TV** 是一款基于 **Tauri 2 + Rust + FFmpeg (VideoToolbox)** 构建的 macOS 原生大屏家庭媒体中心。项目融合了 Apple TV tvOS 的灵动毛玻璃视觉体系、Sony PSP XMB (XrossMediaBar) 的低饱和度动态流光丝带美学，以及日本电视台（NHK / NTV）风格的顶部实时气象速报字幕系统，将**本地高清影视剧集库**、**有线电视 IPTV 直播（含实时 EPG 节目单）**、**抖音多分区原生短视频流**以及**加密私密媒体空间**统一收录于一套丝滑的全键盘/遥控器焦点交互界面之中。

> 🏛️ **特别收录：[《绝境逢生 · 凯旋归来》3D 凯旋门项目纪念网页](./showcase/index.html)**  
> 在浏览器中打开 [`showcase/index.html`](./showcase/index.html)，通过交互式 3D 凯旋门回顾项目负责人高博（Cohiro Gaobo）在代码覆灭危机中临危不乱、足智多谋率领项目逆转重建并胜利凯旋的完整事迹。

---

## 🚀 核心特性

### 1. 🎬 现在观看（本地剧集智能刮削与多季剧集管理）
- **智能文件名清洗与分季聚合**：自动递归扫描本地影视目录，识别 `S01E01`、`Season 1`、`第X季/第X集` 等命名规则，自动归并同名剧集并生成多季胶囊导航栏。
- **TMDB + 豆瓣双评分体系**：自动刮削 TMDB 中文剧名、高清剧照背景、顶格剧情简介与透明 ClearLogo，同时内置直连豆瓣电影评分抓取引擎，支持在偏好设置中一键切换 **TMDB 评分** 或 **豆瓣评分**。
- **正方形/横向 ClearLogo 自适应舞台**：自动检测剧集 Logo 宽高比，针对竖向或正方形 Logo 自动切换至 `148×148` 正方形放置区域放大展示，横向宽幅 Logo 保持经典比例。
- **串行流控视频关键帧提取**：后台通过 Rust 互斥锁与前端队列按序调用 FFmpeg 提取视频真实画面缩略图并持久化缓存，即使面对数百部大体量剧集或私密视频文件也绝不卡顿。

### 2. 📺 有线电视（IPTV 直播、EPG 电子节目单与大屏 HUD）
- **实时 XMLTV EPG 电子节目单**：默认集成 `https://epg.51zmt.top:8000/e.xml` 节目单源（支持在设置中自定义修改），自动解析频道当前正在播出的节目名称并实时展示在频道卡片副标题。
- **台标比例智能微调**：自动识别台标图像宽高比，宽幅台标（如 CCTV 系列）保持原生完美比例，仅对偏方形/偏大台标进行柔和内边距收敛；仅当频道包含实际机构信息时才展示官方蓝 V 认证徽标。
- **大屏专属左下角换台 HUD**：全屏观看直播并上下键换台时，左下角弹出大字号毛玻璃频道与当前节目信息块，贴合客厅远距离观看需求。

### 3. 📱 流媒体（抖音官方多分区无头直通引擎）
- **折叠式二级丸形分类菜单**：默认展示折叠态 `抖音精选 >` 胶囊按钮，点击或按回车即可展开为 **抖音精选、抖音推荐、抖音知识、抖音二次元、抖音游戏、抖音美食、抖音体育、抖音时尚、抖音音乐** 九大官方分区，展开状态下按 `↑` 键即可一键收起。
- **H.264 硬件直通与无头防盗链嗅探**：内置隐藏式 WebKit 嗅探引擎自动拦截过滤 ByteVC1/HEVC/AV1 流，优先提取 H.264 候选地址并通过 `ffprobe` 预检，正式播放页支持 `↑` / `↓` 键无缝连刷切换视频。
- **内置置顶扫码登录**：设置面板提供带抖音音符徽标的 `登录 抖音` 入口，一键唤出置顶登录窗并在完成后自动同步 Cookie。

### 4. 🌦️ 日本电视台风格顶部气象速报与实时特殊天气预警
- **顶部中央双行黑边白字排版**：
  - **第一行居左**：展示常规实况（如 `气象速报 阴 22°C`）或特殊天气提醒；
  - **第一行居右**：展示当前所在区及同城临近三个区（自动去除“区”字后缀，如 `玄武 栖霞 雨花台 秦淮`）；
  - **第二行居右**：展示所属省级/市级气象机构（如 `江苏气象台`）。
- **直连高精度定位（免受代理干扰）**：融合 macOS 原生定位与国内直连 IP 逆地理编码（`--noproxy *`），即便开启规则代理也不会误偏至境外位置。
- **30 分钟常规播报 + 全天候实时预警**：常规气象速报每 30 分钟展示一次；当检测到 **分钟级降水开始、分钟级降水即将停止、分钟级降水结束、空气质量超标（AQI ≥ 101）、高湿预警（湿度 ≥ 85%）或气象灾害预警** 时，立即突破时间限制实时推送。

### 5. 🎨 PSP XMB 动态流光背景与声画控制
- **6 款低饱和度 PSP 流光配色**：基于 Canvas 2D 实时渲染四层正弦谐波流光丝带，内置 **深海幽蓝、暮光星云、竹韵青灰、暖珀流沙、曜石银灰、绯樱夜雾** 六套主题配色。
- **预览声音直出与自由开关**：所有视频卡片焦点预览画面默认支持直接播放声音，无需进入全屏即可试听，并在设置中提供独立开关。
- **窗口模式独立 GPU 合成层**：通过正序图层与 `translate3d` / `backface-visibility` 硬件加速合成，彻底解决 macOS `WKWebView` 窗口缩放模式下背景视频局部切块静止的渲染顽疾。
- **GitHub 离线缓存与一键全量刷新**：自动缓存远程 `config.json` 数据确保断网可用；设置页顶部的同步状态栏可作为焦点按钮直接触发，一键重新载入 GitHub 配置、EPG、本地媒体库、抖音流与天气数据。

---

## 🎮 键盘 / 遥控器操作指南

| 按键 | 主界面导航 | 丸形分类栏 (`#seasonBar`) | 全屏播放模式 |
| :--- | :--- | :--- | :--- |
| `↑` / `↓` | 切换左侧导航栏栏目 / 进出顶部丸形分类栏 | `↑` 收起流媒体二级分类或返回侧边栏；`↓` 回到卡片轨 | 上下无缝切换电视频道 / 抖音视频 / 剧集 |
| `←` / `→` | 在侧边栏与右侧卡片轨道之间移动焦点 | 在展开的分类胶囊之间左右切换 | 切换上一频道 / 下一频道 |
| `Enter` | 进入全屏播放 / 打开多季剧集分集列表 | 展开折叠的流媒体分类菜单 / 确认选择 | 暂停 / 继续播放 |
| `Esc` / `Backspace` | 从剧集分集视图返回剧集海报墙 | 关闭当前弹窗 | 退出全屏播放返回主界面 |
| **长按头像 550ms** | 在左上角用户头像栏按住 `Enter` 或鼠标左键 | 唤出密码窗解锁 / 锁定 **限制级** 私密空间 | — |

---

## 🛠️ 技术架构与运行指南

### 环境依赖
- **macOS** 12.0+（支持 Apple Silicon M 系列与 Intel 芯片）
- **Rust** 工具链 (`rustup` / `cargo`)
- **FFmpeg**（建议通过 Homebrew 安装：`brew install ffmpeg`，自动启用 `h264_videotoolbox` 硬件加速）

### 本地开发与构建

```bash
# 1. 克隆仓库
git clone https://github.com/cohirogaobo/feelcool-tv-mac.git
cd feelcool-tv-mac

# 2. 启动 Tauri 开发调试模式
cd src-tauri
cargo tauri dev

# 3. 打包 macOS 原生应用 (.app / .dmg)
cargo tauri build
```

---

## 📦 相关仓库与链接

- **主工程仓库**：[https://github.com/cohirogaobo/feelcool-tv-mac](https://github.com/cohirogaobo/feelcool-tv-mac)
- **远程配置源仓库**：[https://github.com/cohirogaobo/feelcool-tv-config](https://github.com/cohirogaobo/feelcool-tv-config)
- **3D 凯旋门纪念网页**：[`showcase/index.html`](./showcase/index.html)
