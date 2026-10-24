# Feel Cool TV (macOS Direct Engine · v4.2.0)

<p align="center">
  <strong>专为 macOS 与大屏客厅体验打造的次世代流光家庭媒体中心</strong><br>
  <em>macOS 27 原生 Liquid Glass 液态玻璃 · Apple TV 级深色毛玻璃交互 · 波浪/光影双动态主题 · VideoToolbox 硬件加速 · 抖音 &amp; bilibili 双平台流媒体 · 实时气象预警与 EPG 节目单</em>
</p>

<p align="center">
  <a href="https://github.com/cohirogaobo/feelcool-tv-mac"><img src="https://img.shields.io/badge/Version-4.2.0-0A84FF?style=for-the-badge" alt="Version 4.2.0"></a>
  <a href="https://github.com/cohirogaobo/feelcool-tv-mac"><img src="https://img.shields.io/badge/Platform-macOS%20(Apple%20Silicon%20%26%20Intel)-000000?style=for-the-badge&logo=apple" alt="macOS"></a>
  <a href="https://tauri.app/"><img src="https://img.shields.io/badge/Engine-Tauri%202.0%20%2B%20Rust-FFC131?style=for-the-badge&logo=rust" alt="Tauri 2"></a>
  <a href="https://ffmpeg.org/"><img src="https://img.shields.io/badge/Video-VideoToolbox%20HLS-00D474?style=for-the-badge&logo=ffmpeg" alt="FFmpeg"></a>
  <a href="./showcase/index.html"><img src="https://img.shields.io/badge/3D%20Showcase-%E5%87%AF%E6%97%8B%E7%BA%AA%E4%BA%8B%20(Arc%20de%20Triomphe)-D4AF37?style=for-the-badge" alt="3D Showcase"></a>
</p>

---

## ✨ 项目简介

**Feel Cool TV (`v4.2.0`)** 是一款基于 **Tauri 2 + Rust + FFmpeg + WebKit 原生层扩展 (`libcooltv_glass.dylib`)** 构建的 macOS 全屏客厅家庭媒体中心。项目融合了 **Apple TV (tvOS)** 的深色毛玻璃与长胶囊导航体系、**macOS 27 Golden Gate 100% 原生 Liquid Glass（液态玻璃）** 光学折射材质、**Sony PlayStation / PSP XMB** 的“波浪 / 光影”双动态背景美学，以及日本电视台（NHK / NTV）风格的顶部实时气象速报系统，将**很酷 TV+（本地剧集与电影双库）**、**有线电视 IPTV 直播（含实时 EPG 节目单）**、**流媒体（抖音 + bilibili 双平台）**、**正在观看跨会话断点续播中心**以及**加密限制级私密空间**统一收录于一套丝滑的全键盘/遥控器焦点交互界面之中。

> 🏛️ **特别收录：[《绝境逢生 · 凯旋归来》3D 凯旋门项目纪念网页](./showcase/index.html)**  
> 在浏览器中打开 [`showcase/index.html`](./showcase/index.html)，通过交互式 3D 凯旋门回顾项目负责人高博（Cohiro Gaobo）在代码覆灭危机中临危不乱、足智多谋率领项目逆转重建并胜利凯旋的完整事迹。

---

## 🚀 核心特性 (v4.2.0)

### 1. 💎 双形态侧边栏（左侧 / 顶部 tvOS 胶囊）与原生 Liquid Glass 液态玻璃
- **100% 原生 Liquid Glass 液态玻璃与传统模糊双材质**：通过内置 `libcooltv_glass.dylib` 驱动 WebKit `WKMaterialHostingSupport` 与 `-apple-system-glass-material-clear` 原生托管图层，支持 **Liquid Glass（深色 / 浅色双外观模式）** 与 **传统模糊** 自由切换；正式播放页面的播放状态按钮同样采用原生圆形 Liquid Glass 材质。
- **左侧边栏 & 顶部 tvOS 胶囊侧边栏自由切换**：
  - **左侧边栏**：支持 **自动折叠**、**始终折叠**、**自动隐藏**、**始终隐藏** 四种方案；
  - **顶部侧边栏**：遵循 Apple 官方 tvOS 长连贯药丸胶囊设计，支持 **始终显示** 与 **始终隐藏** 两种方案，进入正式播放页面时自动隐藏；
  - **“始终隐藏”独立分层模式**：开启后无论左侧还是顶部侧边栏均保持隐藏、主内容区全宽不偏移，仅在非正式播放页面按下菜单键（`F16`）时唤出侧边栏并精准聚焦当前所在分区的导航项。

### 2. ▶️ 正在观看 / 现在观看（多分区独立额度与跨会话断点续播）
- **四分区独立丸形菜单**：提供 **很酷 TV+**、**有线电视**、**流媒体**（以及解锁后显现的 **限制级**）独立分类，选中的丸形菜单始终自动对齐当前正在播放的内容分区。
- **各分区独立 20 条历史额度**：很酷 TV+、有线电视、流媒体各自独立享有 20 条观看历史记录配额，互不挤占覆盖；同一部剧集的不同季集自动合并为单条剧集记录并实时跟踪最新观看集数与进度百分比。
- **预览不入库、正式播放才入库**：卡片焦点背景预览不污染历史记录，仅当进入正式播放页面时才写入历史并支持跨会话秒级断点续播。

### 3. 🎬 很酷 TV+（剧集与电影双库分栏与双评分体系）
- **剧集 / 电影二级丸形菜单分栏**：独立配置“剧集资源位置”与“电影资源位置”，自动识别 `S01E01`、`Season 1`、`第X季/第X集` 等命名规则并生成多季选集导航；正式播放页面下按 `F16` 或 `↓` 键唤出底部柔化渐变选集菜单。
- **TMDB + 豆瓣双评分与本地持久化元数据缓存**：自动刮削 TMDB 中文片名、高清剧照、剧情简介与透明 ClearLogo，支持一键切换 **豆瓣评分** 或 **TMDB 评分**，所有海报、Logo 与元数据均持久化缓存至本地磁盘。

### 4. 📱 流媒体（抖音 + bilibili 双平台原生直通引擎）
- **抖音与 bilibili 双平台多分区聚合**：支持 **抖音精选、抖音推荐、抖音知识、抖音二次元、抖音游戏、抖音美食、抖音体育、抖音时尚、抖音音乐** 与 **bilibili 推荐、bilibili 热门、bilibili 动画、bilibili 番剧、bilibili 影视、bilibili 游戏、bilibili 知识、bilibili 科技、bilibili 音乐、bilibili 美食、bilibili 舞蹈、bilibili 运动** 全分区单行横向滚动胶囊菜单，长按任意胶囊或按 `F17` 即可旋转刷新。
- **零切块 CFR 硬件/极速转码管线与原生登录态同步**：通过 `ffprobe` 8-bit H.264 预检、主干 CDN 优选与 `1280×720` 标准宏块对齐转码彻底根除画面切块；后端直接从 `WKHTTPCookieStore` 提取 `SESSDATA` 等 `HttpOnly` Cookie 并配合 Rust 原生 WBI 签名，实现 100% 真实登录态个性化推荐。
- **正式播放页点赞/收藏互动与“只看作者”模式**：在流媒体正式播放页面支持双击 `Return` 点赞、长按 `Return`（或单击 `F17`）收藏，右下角实时展示圆角桃心/大拇指与五角星状态指示器；按 `F16` 唤出底部流媒体选项菜单，一键开启 **“只看作者”** 连刷当前创作者全部作品。

### 5. 📺 有线电视（IPTV 境内/海外智能分流与实时 EPG）
- **境内直连 + 302 预解析与反交错加固**：智能识别境内央视/卫视与海外频道（如新加坡 CNA），境内源自动绕过代理并预解析 HTTP 302 动态节点，配合 `yadif` 反交错滤镜实现秒级起播。
- **实时 XMLTV EPG 电子节目单**：自动解析并展示各频道当前正在播出的节目名称。

### 6. 🌦️ 顶部实时气象信息与 Apple TV 级三级全屏设置系统
- **顶部侧边栏自适应避让气象字幕**：实时监测分钟级降水、空气质量、高湿与气象灾害预警，并根据顶部侧边栏的显隐状态与 UI 缩放比例平滑调整垂直位置，绝不与顶部胶囊侧边栏重叠。
- **Apple TV (tvOS) 深色高透毛玻璃三级设置页、密码页与电源页**：参照 Apple TV `systemThickMaterialDark` 规范采用 `78px` 大半径高斯模糊、`185%` 饱和度增益与多层环境光微调遮罩；内置保留动量的临界阻尼 `rAF` 滚动引擎与上下滚动边缘渐隐遮罩，浏览设置项丝滑自然。

---

## 🎮 键盘 / 遥控器全局快捷键指南

| 按键 | 主页面 / 侧边栏 / 丸形菜单 | 正式播放页面 (`isFullScreen`) | 设置页 / 密码页 / 电源页 |
| :--- | :--- | :--- | :--- |
| `↑` / `↓` / `←` / `→` | 二维十字轴移动焦点（左侧/顶部侧边栏、丸形菜单、卡片轨道） | `↓` 上一频道/上一视频（剧集唤出选集）；`↑` 下一频道/下一视频；`←`/`→` 快退/快进 | 上下丝滑滚动浏览设置项；二级状态项按 `←`/`→` 直接切换状态 |
| `Return (Enter)` | 进入正式播放页面 / 展开剧集选集或流媒体二级分类 | 单击暂停/继续；流媒体双击触发**点赞**、长按触发**收藏** | 进入下一级设置 / 切换状态项 / 确认操作 |
| `Escape` / `Backspace` | 从卡片返回侧边栏（非始终隐藏模式）/ 侧边栏按 `Esc` 兜底回“正在观看”并可切入正式播放页 | 退出正式播放页面返回主页面（保持后台连续播放） | 在三级设置页间逐级返回或关闭弹窗回到主页面（绝不触发退全屏） |
| `F16`（菜单键） | **双向唤出 / 收起侧边栏**（“始终隐藏”模式下唯一唤出侧边栏按键） | 剧集唤出**选集菜单** / 流媒体唤出**流媒体选项（只看作者）** / IPTV 显隐进度条（**严禁唤出侧边栏**） | — |
| `F17`（长按替代键） | 立即触发当前焦点元素的长按动作（如长按头像解锁限制级、长按胶囊刷新流媒体） | 流媒体正式播放页面下直接触发**收藏 / 取消收藏** | 设置一级页显现“限制级内容设置” / 密码页唤出右侧**大九键屏幕数字键盘** |
| `F18`（主页键） | 背景有播放或暂停内容时，一键直达**正式播放页面**并无缝续播 | 一键退回**主页面**并聚焦“正在观看”（后台继续播放不回退进度） | 立即关闭当前弹窗/遮罩并回到主页面“正在观看” |
| `F19`（关闭电视键） | 暂停播放 -> 显示 5 秒“正在关闭电视..”模糊遮罩 -> 3 秒全黑屏 -> 静默回到主页面 | 同左 | 同左 |
| `F20`（亮度调节键） | 唤出底部电视亮度控制条（联动 `BetterDisplay`，`←`/`→` 按 `±10%` 实时调节） | 同左（与播放进度条、选集菜单、流媒体菜单严格互斥替换） | — |

---

## 🛠️ 运行与构建指南

### 直接运行正式版应用包
项目根目录已内置唯一正式版独立应用程序包 **`Feel Cool TV.app` (`v4.2.0`)**，内置完整静态资源服务与 `libcooltv_glass.dylib` 液态玻璃引擎，双击即可直接运行。

### 环境依赖（源码构建）
- **macOS** 12.0+（原生支持 macOS 27 Golden Gate Liquid Glass 与 Apple Silicon M 系列芯片）
- **Rust** 工具链 (`rustup` / `cargo`)
- **FFmpeg**（建议通过 Homebrew 安装：`brew install ffmpeg`，自动启用 `h264_videotoolbox` 硬件加速）
- **BetterDisplay**（可选，安装在 `/Applications/BetterDisplay.app` 时支持 `F20` 硬件级电视亮度调节）

```bash
# 启动开发调试
cd src-tauri
cargo tauri dev
```

---

## 📦 相关仓库与文档

- **技术规范与架构维护手册**：[`TECHNICAL_SPEC.md`](./TECHNICAL_SPEC.md)
- **主工程仓库**：[https://github.com/cohirogaobo/feelcool-tv-mac](https://github.com/cohirogaobo/feelcool-tv-mac)
- **远程配置源仓库**：[https://github.com/cohirogaobo/feelcool-tv-config](https://github.com/cohirogaobo/feelcool-tv-config)
- **3D 凯旋门纪念网页**：[`showcase/index.html`](./showcase/index.html)
