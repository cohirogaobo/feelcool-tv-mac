# Feel Cool TV 技术规范与架构维护手册 (v4.7.0)

本文档面向参与 **Feel Cool TV (`cooltv-mac`)** 后续迭代的开发人员，系统梳理产品的核心调性、视觉与交互设计规范、界面文案措辞准则、全局快捷键与遥控映射体系、**用户对各组件与按键的标准称呼词典**，并重点复盘前端（WebKit / HTML5 / CSS3 / HLS.js）与后端（Tauri v2 / Rust / FFmpeg / Rouille）在历次迭代中遇到的疑难问题、底层成因及最终工程解决方案。

> **版本同步强制规范**：每次完成迭代任务后，必须同步更新产品版本号（当前版本：**`4.7.0`**），涉及位置包括：
> 1. `src/index.html` 与 `Feel Cool TV.app/Contents/Resources/src/index.html` -> `#settingsVersionText` 及 `vFooterEl.textContent`（显示版本号 `4.7.0`）；
> 2. `src-tauri/tauri.conf.json` -> `"version"`（SemVer `4.7.0`）；
> 3. `src-tauri/Cargo.toml` -> `[package] version`（SemVer `4.7.0`）；
> 4. `package.json` -> `"version"`（SemVer `4.7.0`）；
> 5. `Feel Cool TV.app/Contents/Info.plist` -> `CFBundleShortVersionString` 与 `CFBundleVersion`（`4.7.0`）；
> 6. `TECHNICAL_SPEC.md` 与 `README.md` 标题与正文版本记录。
>
> **组件称呼记录强制规范（Round 20 任务 14 新增）**：每次任务结束后，必须在本文档第五章 **“五、组件、页面与快捷键标准称呼词典”** 中详细记录并持续维护用户对每个组件、页面、状态与按键的称呼及对应技术解释。

---

## 一、产品调性与核心定位

1. **10-Foot 客厅大屏沉浸体验（tvOS × PSP XMB 融合美学）**
   - 产品定位于 macOS 全屏客厅媒体中心，融合 **Apple TV (tvOS)** 的毛玻璃层级、呼吸感字重与 **Sony PlayStation / PSP XMB（交叉媒体栏）** 的丝带流光背景及十字轴焦点导航体验。
   - **背景即正式播放舞台**：任何触发了后台视频流预览的画面均视为正在进行的正式播放。只要用户未聚焦到另一个可播放媒体卡片或主动长按停止，焦点在导航栏、丸形菜单或设置项之间移动时**绝不打断当前背景播放**。
   - **全屏模式隐藏鼠标指针、窗口模式允许鼠标指针可见（Round 18 新增，Round 20 & Round 22 演进）**：
     - 当应用处于 **全屏独占模式** 时，严格执行鼠标指针不可见（`html.app-cursor-hidden`，全局 `cursor: none !important`）；
     - 当应用处于 **窗口模式**（含窗口最大化）时，移除 `app-cursor-hidden`，允许鼠标指针正常可见（`syncAppCursorVisibility()` 动态监听原生全屏独占状态）。
2. **全键盘/遥控器优先，二维十字轴精准方向键导航**
   - 所有界面元素均可通过方向键（`ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight`）、确认键（`Enter`）、返回键（`Escape` / `Backspace`）及全局功能键（`F16` / `F17` / `F18` / `F19` / `F20`）完成闭环导航。
   - **丸形菜单（`#seasonBar`）导航与流媒体单行横向滚动遮罩规范（Round 18 ~ Round 23 演进）**：
     - 流媒体二级分类菜单（`douyinPillsExpanded`）强制只显示一行（`.season-bar.single-line-mode`），置于 `.season-pills-scroll-viewport` 内，左右两侧采用跑马灯同款渐变透明遮罩（`linear-gradient(to right, transparent 0px, black 26px, black 93%, transparent 100%)`），当用户左右移动焦点时通过 `scrollSeasonPillIntoView()` 平滑横向滚动菜单；
     - 流媒体左侧刷新元素改为无圆形容器的纯刷新符号状态指示器（`.stream-refresh-indicator`），不再作为可聚焦按钮；**用户长按任何一个丸形菜单项（或聚焦任意丸形菜单时按 `F17`）即可触发刷新当前内容**，刷新期间刷新符号旋转并展开显示“正在刷新”；
     - **“很酷 TV+”二级丸形菜单（Round 23 任务 10 新增）**：“很酷 TV+”页面提供 **“剧集”** 与 **“电影”** 两个二级丸形菜单，默认选中“剧集”；
     - 非单行模式下的多行丸形菜单仍通过 `findSeasonPillInAdjacentRow()` 自动按视觉纵坐标聚类识别多行结构，支持 `ArrowUp` / `ArrowDown` 跨行直达。
   - **第一个卡片返回左侧边栏规范（Round 20 任务 7 新增）**：
     - 任何情况下，当**焦点**位于当前轨道的**第一个卡片**（`col === 1, row >= 1`）时，按下**返回键**（`Escape`、`Backspace` 或向左方向键 `ArrowLeft`）直接返回到**左侧边栏**对应导航项（`col === 0`），绝不返回到上方的**丸形菜单**（`#seasonBar`）。
   - **“正在观看”按钮直达正式播放页面规范（Round 20 任务 6 新增）**：
     - 任何情况下，只要点击或短按确认左侧边栏的 **“正在观看 / 现在观看”** 按钮（`#navNowPlayingItem`，`row === 7, col === 0`），立即进入**正式播放页面**（`enterNowPlayingFullscreen()`）；
     - 如需选取其他历史记录卡片，必须使用**右键**（`ArrowRight`）将焦点从“正在观看”移动到右侧卡片轨道。
   - **多级全屏设置页面与弹窗导航规范（Round 18 ~ Round 23 任务 9 演进）**：
     - 设置页重构为**三级全屏沉浸式架构**：一级分类入口页（“媒体与账户设置”、“个性化设置”，长按任意分类按钮或按 `F17` 唤出“限制级内容设置”） -> 二级分类下设置项列表页（填写性选项右侧显示 `>` SVG，非填写性选项右侧直接显示当前状态名并可通过左右键/回车直接切换状态） -> 三级填写操作页；
     - **密码页（`#passwordModal`）、电源选项页（`#powerModal`）与多级设置页（`#settingsModal`）均全面支持按 `Escape` 键逐级返回或直接关闭回到主页面**。
   - **全局功能快捷键规范（Round 19 ~ Round 23 演进）**：
     - **`F20`（亮度调节键，Round 20 任务 11 新增，Round 23 任务 1 更新）**：按下 `F20` 触发底部**亮度条**（`#tvBrightnessHud`），使用 `ArrowLeft` / `ArrowRight` 每次按 **`±10%`** 步长调节电视亮度（底层调用 `/Applications/BetterDisplay.app/Contents/MacOS/BetterDisplay`），前端以 `0-100` 纯数字显示当前亮度值；**亮度条与视频播放进度条（`#iptvFullscreenHud`）、选集菜单及流媒体选项菜单同时只能二选一显示，严格相互替换**。
     - **`F19`（模拟关闭电视睡眠流程键，Round 19 升级）**：按下 `F19` 不再唤出电源选项页，改为立即触发**强制暂停当前播放内容**，展示与电源选项页同款的模糊色彩遮罩层（`#tvShuttingDownModal`），画面中央以与电源页完全一致的大标题（`38px`）显示 **“正在关闭电视..”**、小标题（`16px`）显示 **“请稍候。”**；计时 **5 秒**后切入全黑屏（`#tvBlackoutCurtain`），黑屏持续 **3 秒**后静默回到主页面，并将焦点移动到 **“现在观看”**（`#navNowPlayingItem`）。
     - **`F18`（主页键，Round 19 & Round 20 任务 2、3 升级）**：
       - 只要当前位于**主页面**（非正式播放页面、非密码页、非设置页、非电源选项页）且**背景有正在播放的内容**，按下主页键（`F18`）立即进入**正式播放页面**，丝滑继续背景内容的当前实时播放进度，绝不打断播放或回退到离开正式播放页面时的旧进度；
       - 在**正式播放页面**按下主页键（`F18`），切换到**主页面**并将焦点移动到“正在观看 / 现在观看”（`#navNowPlayingItem`），背景继续播放并实时同步观看进度；
       - 在密码页、设置页、电源选项页或关机遮罩页按下主页键（`F18`），立即关闭当前弹窗/遮罩，切换到主页面并将焦点移动到“正在观看 / 现在观看”。
     - **`F17`（全局长按替代键 & 流媒体正式播放页收藏键，Round 23 任务 6 演进）**：在主界面/弹窗单击一次 `F17` 即可立即触发当前焦点元素的长按动作；在**流媒体正式播放页面**下单击 `F17`（或长按 `Return` 键）直接触发**收藏/取消收藏**当前视频。
     - **`F16`（全局上下文菜单键 & 本地剧集选集 / 流媒体选项菜单呼出键，Round 18 ~ Round 23 任务 7 演进）**：
       - 全局拦截并禁用 macOS WebKit 底层原生右键菜单；
       - **剧集类视频正式播放页面**下禁止使用上下键（`ArrowUp` / `ArrowDown`）切换上一集/下一集，按下键（`ArrowDown`）或 `F16` 键唤出底部**选集菜单**（`#fsEpisodePickerHud`）；
       - **流媒体视频正式播放页面**下按 `F16` 键唤出底部**流媒体选项菜单**（`#fsStreamMenuHud`，提供带对勾 SVG 激活指示器的 **“只看作者”** 按钮），且流媒体正式播放页上下键方向与 IPTV 统一为：**下键（`ArrowDown`）切换上一个视频，上键（`ArrowUp`）切换下一个视频**。
3. **双空间隐私架构与屏幕数字键盘（常规空间 + 隐藏限制级空间）**
   - 默认界面与设置页对限制级功能实现**零视觉暴露**。
   - 在密码验证页长按密码输入框（或按 `F17`）可唤出/收起右侧大九键屏幕数字键盘（`#pwdNumpadPanel`，含 Apple 规范 `delete.left` 退格键）。
   - **“正在观看 / 现在观看”丸形菜单分组、限制级隐私隔离与清空历史记录（Round 19 ~ Round 23 任务 11、12 演进）**：
     - “正在观看 / 现在观看”页面取消“全部”分类，配备 **很酷 TV+**、**有线电视**、**流媒体**（以及解锁后出现的 **限制级**）独立丸形菜单分类，默认选中“很酷 TV+”；
     - 在“正在观看 / 现在观看”页面下，流媒体项目的**大标题**显示该视频对应的数据源分类名称（如 `抖音精选`、`bilibili 热门` 等，注意 `抖音精选` 已代表抖音含义，严禁同时出现 `抖音 抖音精选` 重复前缀），卡片下方标题与副标题显示单集视频标题和作者；
     - 设置页面提供 **“观看历史记录”**（`清空观看历史记录` 状态项），支持一键清空全部常规与限制级历史观看记录及进度缓存。

---

## 二、视觉与设计规范

### 1. 空间布局、侧边栏折叠方案与全局自适应缩放体系 (`autoScaleUI`)
- **基准视口与左侧边栏折叠方案（Round 18 新增）**：
  - 左侧边栏（`#sidebar`）固定定位于 `left: 20px; top: 20px; bottom: 20px;`，展开宽度 `205px`，折叠宽度 `68px`（`.sidebar.collapsed`）。
  - 在设置页“个性化设置”下提供 **“左侧边栏折叠方案”**（持久化于 `localStorage.fc_sidebar_mode`，默认 **`自动折叠`**）：
    - **`自动折叠` (`auto`)**：焦点在左侧边栏（`col === 0`）时展开，焦点移入右侧内容区（`col >= 1`）时自动折叠为胶囊态。
    - **`始终折叠` (`always`)**：左侧边栏始终保持 `.collapsed` 紧凑图标胶囊样式（主内容区始终 `.expanded`），同时用户依然可以 100% 正常移动焦点至左侧边栏各图标项进行导航、单击或长按操作。
- **全局缩放预设 (`UI_SCALE_PRESETS`)**：
  - 提供 `100%` ~ `200%` 六档缩放，严禁对 `body`、`#macVideo` 或 `#pspWaveCanvas` 应用 `zoom`。

### 2. 色彩系统、柔化底部黑色渐变遮罩与全屏遮罩色调联动
- **PSP XMB 动态丝带背景 (`PSP_BG_THEMES`)**：内置 6 套低饱和度深色主题及 `随机 random` 模式。
- **全屏播放进度条、选集菜单、流媒体选项菜单与亮度条柔化黑色透明渐变遮罩（Round 18 ~ Round 23 升级）**：
  - 全屏播放底部信息与进度条遮罩（`.iptv-fullscreen-hud`）、本地剧集底部选集遮罩（`#fsEpisodePickerHud`）、流媒体选项菜单遮罩（`#fsStreamMenuHud`）以及电视亮度调节遮罩（`.tv-brightness-hud`）统一采用柔化透明度的黑色渐变遮罩：
    `background: linear-gradient(to top, rgba(0, 0, 0, 0.72) 0%, rgba(0, 0, 0, 0.42) 54%, rgba(0, 0, 0, 0) 100%);`
  - **互斥替换原则**：亮度条、播放进度条、选集菜单与流媒体选项菜单同时只能显示其一，唤出任意一个时立即关闭其余遮罩。

### 3. 品牌图片 Logo 规范与字体规范 (Round 18 ~ Round 23 更新)
- **系统主字体栈**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Helvetica Neue", sans-serif`。
- **“很酷 TV+”内联图片 Logo 与视觉字高对齐规范（Round 21 ~ Round 23 任务 10 更新）**：
  - 界面大标题、丸形菜单及副标题中的 `"很酷 TV+"` 统一使用四周无留白斜体透明白色图片 Logo（`<img class="inline-cooltv-logo" src="./assets/cooltv_plus.png" alt="很酷 TV+">`）内联渲染；
  - 当焦点位于“很酷 TV+”侧边栏或“剧集 / 电影”丸形菜单时，大标题显示为 `"很酷 TV+ 剧集"` 或 `"很酷 TV+ 电影"`；`.inline-cooltv-logo` 设置为 `height: 0.97em; vertical-align: -0.11em;`，使其主笔画视觉高度与后方的 `"剧集"` / `"电影"` 二字完全一致且避免过大。
- **`bilibili` 全局图片 Logo 替换与空格规范（Round 20 ~ Round 22 更新）**：
  - 界面中所有 `bilibili` 字样统一使用透明白色图片 Logo（`<img class="inline-bili-logo" src="./assets/bilibili.png" alt="bilibili">`）代替，无论在丸形菜单还是正文/副标题中，Logo 与后续分类名之间均保留一个空格（如 `bilibili 热门`）。
- **抖音美好体 (`DouyinMeihaoti` / `.douyin-font`)**：单独出现时不加空格，嵌入其他文字中时通过 `formatDouyinBrandTextHtml()` 在两侧自动加空格；流媒体分类名（如 `抖音精选`、`抖音推荐`）本身已包含抖音品牌含义，严禁在前方重复拼接 `抖音`。
- **气象速报字体与字号规范 (`#jpWeatherTelop`，Round 18 更新)**：
  - 使用标准系统无衬线字体栈；主行字号 `33px`，次行发布机构字号 `27px`。

---

## 三、用户交互内容的文字措辞规范

1. **严禁出现按键括号提示或机械操作说明**
2. **多级设置页文案与层级命名规范（Round 23 任务 9、10 更新）**
   - 一级页面主标题：**`设置`**
   - 一级分类按钮：**`媒体与账户设置`**、**`个性化设置`**、**`限制级内容设置`**（默认隐藏，长按一级分类按钮或按 `F17` 显现）
   - 二级设置项名称与类型：
     - **媒体与账户设置**：`数据同步`（状态项）、`剧集资源位置`（`>` 三级填写页）、`电影资源位置`（`>` 三级填写页）、`EPG 电子节目单来源`（`>` 三级填写页）、`短视频账号连接`（`>` 三级页含 `登录 抖音`、`登录 bilibili`）、`虚拟专用网络代理`（状态项：`开启` / `关闭`）、`观看历史记录`（状态项：`清空观看历史记录`）、`本地缓存数据`（状态项）
     - **个性化设置**：`气象信息定位`（状态项：`自动定位` / `手动输入`）、`所在地位置`（`>` 三级填写页：`城市`、`区`、`街道`）、`影片评分来源`（状态项：`豆瓣` / `TMDB`）、`背景主题`（状态项：`波浪` / `光影`）、`背景配色方案`（状态项）、`用户界面显示比例`（状态项）、`侧边栏位置`（状态项：`左侧` / `顶部`）、`侧边栏材质设计`（状态项：`Liquid Glass` / `传统模糊`）、`Liquid Glass 外观模式`（状态项：`深色` / `浅色`）、`左侧边栏折叠方案`（状态项：左侧模式下为 `自动折叠` / `始终折叠` / `自动隐藏` / `始终隐藏`；顶部模式下为 `始终显示` / `始终隐藏`）、`交互音效`（状态项：`开启` / `关闭`）、`交互音效方案`（状态项：`深海流光` / `星云潮汐` / `深邃脉冲`）、`交互音效音量`（状态项：`20%` / `40%` / `60%` / `80%` / `100%`）、`预览时开启声音`（状态项：`开启` / `关闭`）、`音量均衡`（状态项：`开启` / `关闭`）
     - **限制级内容设置**：`限制级媒体资源位置`（`>` 三级填写页）、`限制级内容密码`（`>` 三级填写页）
   - 一级页面最底部版本号：**只显示纯版本号数字**（当前为 `4.7.0`）。

---

## 四、前端与后端代码注意事项（历史疑难问题与最终解决方案）

本节汇总了项目开发过程中曾导致严重视觉异常、卡顿或功能失效的代码细节及其根因与最终方案，后续开发人员在修改相关模块时**切勿回退或破坏以下机制**：

### 1. WebKit `AVSampleBufferDisplayLayer` 视频切流与几何过渡白屏/闪烁问题
- **底层原因与解决方案 (`src/index.html` -> `hideVideoLayerInstant` / `loadAndPlayVideo`)**：
  1. **严禁使用 `opacity: 0` 或 `visibility: hidden` 隐藏视频层**：隐藏视频时必须设置 `macVideo.style.opacity = '0.002'` 且保持 `visibility: visible`，保持 WebKit 硬件解码管线与 GPU 合成层持续活跃。
  2. **几何稳定延迟显影（380ms Settle Window）**：视频触发 `playing` 且 `readyState >= 2`、`videoWidth > 0`、`currentTime >= 0.16` 后，额外等待 `380ms` 再平滑显影至 `1`。

### 2. 流媒体（抖音 & bilibili）进入播放时偶发“先出现几秒黑屏假视频，进度条走完才进入真正视频”问题（Round 17 重点复盘）
- **历史异常表现**：
  - 播放抖音系或 bilibili 系视频时，有一定概率进入全屏后会先出现几秒钟的黑屏假视频文件，底部进度条显示 `00:00 / 00:02`（或数秒）并快速走完，等进度条走完之后才会真正进入正片视频。
- **底层根因（三重叠加）**：
  1. **`-c:v copy` 遭遇线上源长 GOP 与 MP4 EditList (`elst`) 偏移**：
     - 抖音和 bilibili 的线上 MP4/FLV 视频普遍带有 `5s ~ 8s` 的长 GOP 以及 B 帧 EditList 时间戳偏移（实测 `ffprobe` 抓取到 `-c:v copy` 生成的首分片 `seg_0000.ts` 存在 `start_time = 1.466s`、`duration = 7.0s`），部分流甚至在拷贝完首个分片后即因时间戳/比特流不兼容提前退出，留下一个仅有几秒的黑屏残片 `.m3u8`。
  2. **预览流与全屏流并发竞态导致前序 FFmpeg 被杀**：
     - 用户将焦点移到卡片满 `320ms` 触发 `startPreviewStream`（启动 FFmpeg 进程 #1 并挂载 `hlsInstance` #1，但此时尚在 `380ms` 显影等待期内，`isVideoReady` 仍为 `false`）。此时用户按回车进入全屏，原 `executeItemAction` 因判断 `!isVideoReady` 为真，未复用已在工作的 `hlsInstance` #1，而是将画面置黑（`setVideoLayerVisible(false)`）并发起第二个 `start_ffmpeg`（进程 #2）。
     - 进程 #2 在后端立即杀死了进程 #1（此时进程 #1 仅生成了 1~2 个分片共 2~4 秒），而前端未销毁 `hlsInstance` #1，`macVideo` 仍在黑屏下继续播放 `hlsInstance` #1 残留的 2~4 秒分片并将进度条推至 100%，直到进程 #2 切片就绪后才加载出真正的视频！
  3. **`trueMediaDuration === 0` 期间误将 HLS 初始分片缓存时长当成总时长**：
     - 在异步 `get_media_duration` 返回前（`trueMediaDuration === 0`），原 `getEffectiveDuration()` 直接返回 `macVideo.duration`（即 HLS `EVENT` 播放列表当前已生成的 1~3 秒分片时长），导致进度条显示 `00:00 / 00:02` 并在 `timeupdate`（`effCur >= effDur - 0.18`）中误触发自动切台。
- **最终解决方案 (`src-tauri/src/lib.rs` & `src/index.html`)**：
  1. **废除流媒体 `-c:v copy`，全面采用 `h264_videotoolbox` 硬件重整 1 秒标准 GOP**：
     - 在 `build_ffmpeg_args` 中，流媒体统一经过 `h264_videotoolbox`（失败回退 `libx264`）配合 `-g 30 -hls_time 1 -hls_flags independent_segments` 输出，确保第 0 秒起即具备标准 IDR 关键帧与 `start_time = 0.0s`。
     - 在 `start_ffmpeg` 轮询中，若子进程非零退出（`!status.success()`），严禁返回残损的 `.m3u8`，立即清理并切换下一候选源。
  2. **同源 `hlsInstance` / `inFlightFfmpegReq` 无缝复用 + 异源切流前强制清空旧实例 (`destroyActiveHlsAndResetVideo`)**：
     - 在 `executeItemAction` 中，只要 `currentBgUrl === url && !isPausedIptv && (inFlightFfmpegReq || hlsInstance)`，无论 `isVideoReady` 是否已完成 380ms 显影，均直接复用当前流管线并开启声音，绝不二次重启 FFmpeg；
     - 当真正切换到不同 URL 时，在调用 `invoke('start_ffmpeg')` 前立即调用 `destroyActiveHlsAndResetVideo()` 销毁旧 `hlsInstance` 并暂停 `macVideo`，杜绝旧流在黑屏幕后偷跑进度。
  3. **内联毫秒级时长透传 (`extractInlineDuration`) + 未就绪时屏蔽临时 HLS 缓冲时长**：
     - 抖音（`@@dur={sec}`）与 bilibili（`bvid@@cid@@{sec}`）均在 URL 中直接内联携带精确秒数，前端 `probeStreamDuration` 通过 `extractInlineDuration(url)` 在 `0ms` 内同步写入 `trueMediaDuration`；
     - `getEffectiveDuration()` 在 `trueMediaDuration <= 0` 且视频未结束时，拒绝采信 `< 15s` 的 HLS 临时缓冲时长；`timeupdate` 仅在 `isVideoReady && trueMediaDuration > 2.5` 时才允许触发连播切换。

### 3. 抖音推荐流加载极慢（Task 1）、部分卡片无法播放（Task 9）、推荐流尾部总是出现相同固定内容（Task 10）的三重根因与终极修复（Round 17 重点复盘）
- **历史异常表现**：
  1. 冷启动时抖音推荐流需要极长时间才能加载出第一张卡片；
  2. 加载出的部分抖音卡片点击后黑屏无法播放；
  3. 每次重启或刷新后，抖音推荐流前半部分是新的，但**尾部总是出现完全相同的一批固定视频**。
- **底层根因**：
  1. **为什么首屏加载极慢（Task 1）？**
     - **根因 A（`__FC_LOAD_MORE__` 1.6s 自杀式 `window.location.reload()` 循环）**：抖音网页版首屏 JS 包体较大，加载需 2~4 秒。而此前 `initApp()` 与 `650ms` 轮询在启动瞬间连续调用 `__FC_LOAD_MORE__()`，其内部的 `1600ms` 定时器在 `t ≈ 2.2s` 时检测到 `__FC_SEEN__.size === 0`，误判为页面卡死并执行 `window.location.reload()`，生生打断正在加载的首屏请求并陷入反复重载死循环！而当用户打开抖音登录窗时，因注入了 `#fc-login-bar` 跳过了 `reload()` 判断，页面才得以加载完成。
     - **根因 B（macOS 隐藏 `WKWebView` 挂起原生 `requestAnimationFrame`）**：`douyin_scraper` 窗口处于 `.visible(false)` 隐藏态，macOS WebKit 会直接挂起隐藏窗口的 `window.requestAnimationFrame` 回调，导致依赖 `rAF` 的抖音 React 首屏水合与列表挂载陷入停滞。
     - **根因 C（`ffprobe` 未绕过系统代理且探测窗口过大）**：`probe_douyin_candidates` 未剥离 `http_proxy`/`https_proxy` 环境变量，且 `probesize` 设为 `1.5MB`，导致每个视频探测耗时过长。
  2. **为什么部分抖音卡片无法播放（Task 9）？**
     - Round 16 为追求卡片快出，在 `on_document_title_changed` 中未等 `probe_douyin_candidates`（`ffprobe`）完成就将原始 URL 直接推入 `douyin_feed`，导致抖音返回的 `ByteVC2 (bvc2)` 编码视频或失效 CDN 地址绕过门禁成为卡片。
  3. **为什么每次推荐流尾部都有相同的一批固定内容（Task 10）？**
     - 抖音 `/discover` 页面 HTML 在服务端静态预构建了一批固定的 DOM 节点（`[data-aweme-id]`，即 `jingxuan_pre_build` 静态预构建卡片）以及 `_SSR_DATA` / `RENDER_DATA` 静态快照。
     - 页面加载时，实时推荐 API（`/aweme/v1/web/tab/feed/`）率先返回新鲜视频进入队列前半部；随后 `300ms` 定时器扫描 DOM 中的静态 `[data-aweme-id]` 并逐个调用 `window.jingxuan_pre_build_util.fetchAwemeInfo` 慢速拉取，耗时 3~6 秒后恰好全部追加在实时推荐流的**尾部**，导致无论如何重启或刷新，尾部永远出现这批相同的静态预构建视频！
- **最终解决方案 (`src-tauri/src/lib.rs` -> `ensure_douyin_scraper` / `probe_douyin_candidates`)**：
  1. **彻底移除 `jingxuan_pre_build_util.fetchAwemeInfo` 静态 DOM 抓取器与 SSR 静态快照扫描**：仅拦截实时网络接口（`fetch` / `XMLHttpRequest` / `Response.prototype.json` / 网络 `JSON.parse`，显式排除 `#RENDER_DATA` 与 `_SSR_DATA`），并在每次启动/刷新时调用 `purgeDouyinFeedStorage()` 清空本地预构建缓存，彻底根除尾部固定重复内容。
  2. **移除首屏自杀式 `reload()` + 16ms 定时器接管隐藏窗口 `requestAnimationFrame`**：
     - 在隐藏窗口中将 `window.requestAnimationFrame` 重写为 `setTimeout(cb, 16)`，伪装 `1920×1080` 视口与 `[native code]` 签名，并定时唤醒 `__pc_pace_load_deferred_scripts()`；
     - 在 `__FC_LOAD_MORE__` 中移除自动 `window.location.reload()`，且在页面导航前 `2600ms` 内禁止触发下拉滚动干扰首屏请求。
  3. **恢复严格的 `ffprobe` 编码预检门禁 + 直连 500KB 极速探测**：
     - `probe_douyin_candidates` 注入 `apply_no_proxy_env` 强制直连国内 CDN，将 `analyzeduration` 与 `probesize` 降至 `500000 (500KB)`，单条探测仅需 `~180ms`；
     - 严格要求只有通过 `ffprobe` 校验且非 `bvc2` / `bytevc2` / `av01` 的视频才允许推入 `douyin_feed`，确保展示出的每一张抖音卡片 100% 可正常播放。

### 4. 中国国内地址 IPTV 频道黑屏无法播放（仅新加坡频道可播）问题（Round 17 重点复盘）
- **历史异常表现**：
  - 有线电视（IPTV）板块中，中国国内地址的频道（CCTV 及各省卫视）全部黑屏无法播放，而新加坡频道（`CNA 亚洲新闻台`）可以正常播放。
- **底层根因**：
  1. **FFmpeg 继承系统代理导致国内源被海外出口阻断**：开启代理软件时，`Command::new(ffmpeg_bin)` 继承了 `http_proxy` / `https_proxy` / `all_proxy` 环境变量。新加坡 `CNA` 频道走海外代理可通，而中国国内 IPTV 直播源（中移动 `ottrrs.hl.chinamobile.com`、咪咕、央视及各省广电源）对海外代理 IP 直接返回 `403 Forbidden` 或连接超时。
  2. **缺少浏览器 `User-Agent`、未解析 `HTTP 302` 跳转真实地址及 `-reconnect_streamed 1` 冲突**：
     - 原 IPTV 分支未设置 `-user_agent`，默认发送 `Lavf/*` 被国内 CDN 拦截；
     - 国内 `PLTV` 等直播源大量使用 `HTTP 302` 跳转至动态节点 IP，FFmpeg 直接打开初始地址会导致子分片相对路径解析错误；且 `-reconnect_streamed 1` 会导致部分国内 HLS 直播流在刷新 `.m3u8` 时挂起；
     - 国内电视频道多为 `1080i` 隔行扫描且部分频道音轨特殊，缺少 `-map 0:v:0 -map 0:a:0?` 与 `yadif=0:-1:1` 反交错滤镜会导致 `h264_videotoolbox` 或音频滤镜初始化失败。
- **最终解决方案 (`src-tauri/src/lib.rs` -> `resolve_iptv_stream_candidates` / `build_ffmpeg_args`)**：
  1. **智能境内/海外分流与 `302` 真实地址预解析**：
     - `resolve_iptv_stream_candidates` 根据频道标题与域名识别境内频道与海外频道（如 `CNA` / `NTV`）；
     - 对境内频道，先通过 `curl -sL --noproxy "*"`（携带浏览器 UA）在 `2s` 内解析出 `HTTP 302` 跳转后的最终 `url_effective`，并在启动 FFmpeg 时调用 `apply_no_proxy_env` 强制直连（同时保留带代理与软解 `libx264` 作为多级兜底尝试）。
  2. **IPTV 专用流式转码参数加固**：
     - 注入标准 Safari `User-Agent`，移除 `-reconnect_streamed 1`，添加 `-map 0:v:0 -map 0:a:0?`、`-vf yadif=0:-1:1,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p` 及 `-hls_time 1 -g 30`，实现境内与海外 IPTV 频道秒级起播。

### 5. 标题单行跑马灯 (`smoothMarquee`) 左侧遮罩遮字与循环结束闪烁问题
- **最终解决方案 (`src/index.html`)**：
  1. **负外边距左移遮罩区**：为 `.single-line-marquee` 设置 `margin-left: -26px; padding-left: 26px; -webkit-mask-image: linear-gradient(to right, transparent 0px, black 26px, black 93%, transparent 100%);`。
  2. **等宽 Flex 双轨 + GPU 合成层锁定 + DOM Diff 防重建**：`.marquee-track` 采用 `display: inline-flex; width: max-content; transform: translate3d(0,0,0); backface-visibility: hidden;`，并通过 `data-marquee-key` 避免轮询重建 DOM。

### 6. 气象速报彩色下划线遮字、VPN/TUN 代理误定位“阿姆斯特丹”与自动/手动双模定位机制
- **最终解决方案 (`src/index.html` & `src-tauri/src/lib.rs` -> `fetch_weather_broadcast`)**：
  1. **下划线置后**：`#telopMainText` 设置 `position: relative; z-index: 1; isolation: isolate;`，彩色下划线通过 `::after` 伪元素（`z-index: -1`）置于文字涂层正后方。
  2. **自动定位与手动输入（城市、区、街道）双模架构 + 境内三重校验**：自动定位模式下所有坐标与地名必须通过 `is_china_coord (16°N~54°N, 73°E~136°E)`、`is_china_country` 与 `!is_foreign_place_name` 校验，一旦检测到境外代理节点立即回退至手动保存位置或国内默认基准位置。

### 7. 电源选项“重新启动”白屏问题复盘与进程内热重启方案
- **最终解决方案 (`src-tauri/src/lib.rs` -> `restart_app` & `src-tauri/tauri.conf.json`)**：
  - 在 `tauri.conf.json` 中显式声明 `"backgroundColor": "#000000"`；`restart_app` 采用进程内原子化热重启（递增 `ffmpeg_generation`、终止 FFmpeg、清理 `/tmp/cooltv_hls`、清空推荐流缓存并重载主窗口），彻底消除端口冲突与白屏。

### 8. bilibili 登录提示“浏览器版本过低”与抖音推荐流混入用户“曾经点过赞”重复视频问题（Round 18 重点复盘）
- **历史异常表现**：
  1. **bilibili 登录入口提示“浏览器版本过低”**：在偏好设置页点击“登录 bilibili”后，打开的窗口跳转到 `/blackboard/fe-activity-low-version.html`（提示“浏览器版本过低，建议升级浏览器或下载哔哩哔哩桌面端”），无法正常扫码或输入账号登录。
  2. **抖音推荐流反复混入用户曾经点过赞的视频**：登录后，抖音推荐流中经常出现一批重复视频，经排查全部是当前账号曾经点过赞（或收藏）的内容。
- **底层根因**：
  1. **Tauri `WKWebView` 默认 User-Agent 缺少现代浏览器版本标识**：`WebviewWindowBuilder::new(app, "bilibili_scraper", ...)` 未显式设置 `.user_agent(...)`，macOS `WKWebView` 默认发送不含 `Version/17.x Safari/605.1.15` 或 `Chrome/128.0.0.0` 的精简 `AppleWebKit/605.1.15` UA，触发了 bilibili 前端路由的低版本浏览器拦截重定向。
  2. **抖音网页端后台拉取个人点赞/收藏接口且 `deepScan` 无差别递归**：用户登录抖音后，网页端会在后台自动请求 `/aweme/v1/web/aweme/favorite/`、`/listcollection/`、`/history/` 等接口或返回包含 `user_digged === 1`（或 `status.is_digged === true`）的数据块，此前 `XMLHttpRequest` / `fetch` / `Response.prototype.json` 钩子未过滤上述个人列表接口且 `recordAweme` 未排除已点赞项。
- **最终解决方案 (`src-tauri/src/lib.rs` -> `ensure_bilibili_scraper` / `ensure_douyin_scraper`)**：
  1. **显式注入现代桌面 Chrome 128 User-Agent + 登录直达路由与低版本页自动纠偏**：
     - 在 `bilibili_scraper` 与 `douyin_scraper` 的 `WebviewWindowBuilder` 上统一调用 `.user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36")`；
     - `bilibili_scraper` 初始地址直达 `https://passport.bilibili.com/login`，并在 `bili_hook_script` 中同步伪装 `navigator.userAgent` / `window.chrome` 且实时检测 `low-version` URL 自动重定向至标准登录页。
  2. **URL 接口级 + 对象属性级 + 字段级三层过滤排除已点赞/收藏视频**：
     - 新增 `isExcludedDouyinUrl(u)` 拦截所有匹配 `/favorite|digg|like|collection|collect|history|visited|familiar|following|user\/post|aweme\/post|follow\/feed/i` 的请求；
     - 在 `deepScan` 中跳过 `favorite_list`、`digg_list`、`like_list`、`collect_list`、`history_list` 键；
     - 在 `recordAweme(aweme)` 中严格校验 `aweme.user_digged === 1 || aweme.is_digged === 1 || aweme.collect_stat === 1 || aweme.user_favorited === 1 || (aweme.status && aweme.status.is_digged)`，命中即直接丢弃，彻底杜绝已点赞内容混入推荐流。

### 9. 正在观看/现在观看持久化记忆（上限 20 条）、卡片底部贴边白线进度条与“预览不入库、正式播放才入库”机制（Round 18 & Round 19 重点复盘）
- **核心设计规范 (`src/index.html`)**：
  1. **未进入正式播放的预览内容绝不计入历史记录 (`startPreviewStream` vs `executeItemAction`)**：
     - 焦点停留在卡片上触发的背景预览（`startPreviewStream`）仅调用 `syncNowWatchingRowData(false)`，使当前正在预览的内容临时作为“正在观看”页的第一张卡片展示；一旦用户预览其他内容或停止预览，该未进入正式播放的临时记录自动消失；
     - 只有当用户按回车/点击真正进入**正式全屏播放页面**（`isFullScreen = true`）时，才调用 `recordWatchHistoryItem(activePreviewMeta)` 将记录写入 `watchHistoryList`（或限制级独立列表 `restrictedWatchHistoryList`）并持久化。
  2. **跨会话记忆保存、最多 20 条上限与观看进度精准恢复（Round 19 修复 Task 3）**：
     - **历史卡片无法跳转保存进度的根因与修复**：
       - **根因 A**：此前 `updateActiveWatchHistoryProgress()` 在非全屏背景预览（`!isFullScreen`）的 `timeupdate` 事件中也会执行，导致用户聚焦到“正在观看 / 现在观看”的历史卡片触发背景预览时，初始 `0s~2s` 的预览时间戳直接覆盖了 `watchHistoryList` 中已保存的 `progressSec`；
       - **根因 B**：当背景预览流已在播放该 URL 时，按回车触发 `executeItemAction()` 命中了 `currentBgUrl === url` 的流复用分支提前返回，跳过了后续的 `savedHistEntry.progressSec` 起播检查。
       - **修复方案**：`updateActiveWatchHistoryProgress()` 严格限制仅在 `isFullScreen === true` 时才允许更新已保存的历史进度；同时在 `startPreviewStream()` 与 `executeItemAction()` 中均通过 `findSavedWatchHistoryEntry()` 读取历史 `progressSec`，并在复用预览流时校验当前播放秒数是否已对齐 `resumeSec`（若偏差超过 `4.5s` 则立即通过 `seekSec` 重新定位起播）。

### 10. 部分流媒体视频画面卡顿、仅暂停才刷新画面及标题背景画面不同步问题（Round 19 重点复盘 Task 5）
- **历史异常表现**：
  - 部分流媒体（抖音 / bilibili）视频播放时画面卡顿难以正常推进，每次触发暂停才会使视频画面变化一帧，且主界面标题文字区域的背景画面出现撕裂/不同步。
- **底层根因**：
  1. **`+igndts` 叠加 `-fps_mode passthrough` 破坏含 B 帧视频流的解码时序**：抖音与 bilibili 高码率视频流普遍包含高阶 B 帧重排（PTS 与 DTS 不一致）或可变帧率（VFR）。此前 FFmpeg 输入端开启了 `-fflags +genpts+discardcorrupt+igndts` 并配合 `-fps_mode passthrough`，丢弃原始 DTS 后直接透传时间戳会导致生成的 HLS 分片出现非单调 DTS 跳变，macOS WebKit `AVSampleBufferDisplayLayer` 硬件渲染队列因等待参考帧时间戳而冻结，仅在触发 `pause()` 强制刷新渲染管线时才吐出一帧。
  2. **前端对 MSE `<video>` 挂载 `AudioContext.createMediaElementSource` 劫持媒体时钟**：前端在 `hls.js` MediaSource Extensions (`MSE`) 视频元素上重复挂载了 Web Audio API 动态压缩节点，在 macOS `WKWebView` 下会劫持 MSE 音频主时钟，并在音频采样率切换时引发音画时钟死锁。
- **最终解决方案 (`src-tauri/src/lib.rs` & `src/index.html`)**：
  1. **后端 FFmpeg 恒定帧率重采样与零 B 帧低延迟编码 (`build_ffmpeg_args`)**：
     - 移除 `+igndts`，将 `-fps_mode passthrough` 改为 `-fps_mode cfr -r 30`；
     - 视频滤镜统一限制在 `scale='min(1920,iw)':'min(1080,ih)'` 并串联 `fps=30,format=yuv420p`，编码端指定 `-profile:v main -bf 0`（禁用 B 帧重排缓冲），音频滤镜前置 `aresample=async=1:first_pts=0` 自动補償首帧时间戳偏移。
  2. **前端移除 `createMediaElementSource` 与 `crossOrigin` 隔离**：由后端 FFmpeg 的 `acompressor` + `alimiter` 统一负责响度均衡，前端直接通过 `macVideo.volume` 控制增益，彻底消除 WebKit MSE 时钟死锁与标题区图层撕裂。

### 11. 滚动标题（Marquee）滚回静止位时的瞬时闪烁根因与双节点乒乓驻留方案（Round 19 重点复盘 Task 10）
- **底层根因**：
  - 原 CSS `@keyframes smoothMarquee` 在 `100%`（`translate3d(-50%, 0, 0)`）切换到下一周期 `0%`（`translate3d(0, 0, 0)`）的瞬间，将原本处于屏幕可视区 `x = 0` 的第 2 个 `<span>` 瞬间替换为刚在左侧遮罩外（`-50%`）被 WebKit 视口剔除（Offscreen Culling）的第 1 个 `<span>`，GPU 纹理重上传造成明显的 1 帧闪烁。
- **最终解决方案 (`src/index.html` -> `startMarqueePingPong`)**：
  - 废除 CSS Keyframes 归零跳变，改用 `requestAnimationFrame` 驱动的双节点乒乓驻留引擎（`span.marquee-copy-a` 与 `span.marquee-copy-b`）：当后置节点从右侧（`+stride`）平滑滚动抵达静止位（`0px`）时，**不执行归零替换，而是直接让该可见节点原地驻留（`0px`）成为下一周期的前置静止节点**，仅将已经完全滑出左侧遮罩外的旧节点在幕后移至右侧（`+stride`）待命，实现静止位零 DOM/纹理切换与零闪烁。

### 12. 主页键（F18）在正式播放页面与主页面间切换丢失背景播放进度及剧集/流媒体跨类型连播问题（Round 20 重点复盘 Task 2、3、8）
- **历史异常表现**：
  1. **F18 返回正式播放页面时丢失背景播放进度**：在正式播放页面按下主页键（`F18`）返回主页面后，视频在主页面背景继续播放推进；但当再次按下 `F18`（或点击“正在观看”）返回正式播放页面时，播放进度会倒退回离开正式播放页面时的旧进度。
  2. **剧集类视频通过“正在观看”回看完毕后误播其他类型视频**：当用户从“正在观看”记录点播某部剧集，单集播放完毕（`ended`）后，原逻辑按 `fullScreenRow === 7` 的卡片顺序自动播放了“正在观看”清单中的其他类型视频。
- **底层根因与最终解决方案 (`src/index.html`)**：
  1. **实时同步背景播放进度 + 同源活动流零重连复用 (`updateActiveWatchHistoryProgress` / `executeItemAction`)**：
     - 原 `updateActiveWatchHistoryProgress()` 限制了 `if (!isFullScreen) return;`，导致视频退回主页面后台继续播放期间，`watchHistoryList` 与 `activePreviewMeta.progressSec` 停留在离开正式播放页面时的旧秒数；再次按 `F18` 进入 `executeItemAction()` 时，因检测到当前播放秒数 `curEffSec` 与旧 `resumeSec` 差值超过 `4.5s`，误触发了 `needJumpToSavedProgress` 重新拉起 FFmpeg 跳回旧进度；
     - **修复方案**：允许 `updateActiveWatchHistoryProgress()` 在 `isFullScreen || (isVideoReady && !macVideo.paused)` 期间持续同步实时播放进度（同时保留 `histItem.progressSec > 5 && curSec < 3.5` 防覆盖门禁）；并在 `executeItemAction()` 中识别 `isReusingActiveBgStream`，只要同一视频流已在背景正常播放推进，直接以当前实时秒数 `curEffSec` 为准并复用现有 `hlsInstance`，绝不重启 FFmpeg 或回退进度。
  2. **剧集与流媒体完播同源连播隔离 (`playNextLocalSeriesEpisodeIfAvailable` / `triggerDouyinAutoNextIfNeeded`)**：
     - 在 `macVideo` 的 `ended` 事件中，优先调用 `isCurrentStreamLocalSeries()` 检测当前视频是否属于本地剧集；若属于剧集（无论是否从“正在观看”进入），强制调用 `playNextLocalSeriesEpisodeIfAvailable()` 在 `SERIES_GROUPS[seriesKey]` 内自动衔接下一集（或下一季第一集），绝不按“正在观看”清单播放其他视频；
     - 同理，流媒体视频（`isCurrentStreamDouyin()`）播放完毕后，强制锁定在 `ROW_DATA[3]` 流媒体列表内连播，严禁跨类型衔接其他内容。

### 13. 亮度条实时响应、密码页 Esc 键全局失效、设置页焦点行序错位与返回键兜底连贯切换（Round 21 重点复盘 Task 1–10）
- **历史异常表现与优化诉求**：
  1. **亮度条滞后回跳（Task 1 & 2）**：由于每次调用 `BetterDisplay` shell 命令等待电视响应耗时较长，快速连按左右键调节亮度时，延迟返回的旧硬件亮度值会覆盖前端滑杆，导致滑杆慢一拍或回跳；同时亮度符号外层的圆形按钮容器冗余。
  2. **很酷 TV+ 与 bilibili Logo 显示及绘制瑕疵（Task 3, 4, 5, 6）**：“很酷 TV+”需改为与流媒体“抖音”、“bilibili”标题同款的内联显示方式并采用原版斜体、四周零留白；`bilibili` Logo 中 `b` 字母下半圆偏高需压扁并与原版对齐；`bilibili` 丸形菜单内需改为 `"bilibili XX"`（加入空格）；所有 TMDB 剧集 Logo 需缩小 20%。
  3. **密码页按下 Esc 键导致全局 Esc 失效（Task 7）**：macOS `WKWebView` 中聚焦原生 `<input type="password">` 时按下 `Esc` 会触发 Cocoa `cancelOperation:` 与安全输入接管异常，导致此后 `Esc` 键在全局任何场合失效。
  4. **设置页方向键在“音量均衡”处跳回第五行“系统 VPN”（Task 8）**：`getSettingsModalRows()` 中将 `btnToggleSystemVpn` 与 `btnClearWatchHistory` 排在了 `btnToggleVolumeNorm` 之后，与 DOM 实际从上到下的视觉顺序（第 5、6 行）不一致。
  5. **整点/半点时钟文字阴影聚集（Task 9）与返回键侧边栏兜底往返切换（Task 10）**：右上角整点/半点时钟阴影需增大模糊半径以柔化过渡；侧边栏需支持按 `Esc` 兜底回到“正在观看/现在观看”，并在“正在观看”与正式播放页面间使用 `Esc` 丝滑往返切换且不丢失播放进度。
- **底层根因与最终解决方案**：
  1. **亮度条乐观实时更新 + 后端命令串行下发 (`stepTvBrightness` / `adjust_tv_brightness`)**：前端按左右键时立即按 `±20%` 实时更新滑杆与数值，记录 `lastTvBrightnessManualStepAt` 屏蔽滞后返回值的覆盖，并通过 `tvBrightnessCmdChain` 串行下发 `BetterDisplay set -offset`（后端移除二次 `get` 阻塞查询）；亮度图标改为 `.tv-brightness-icon` 无框直接展示。
  2. **品牌 Logo 零留白矢量重绘与内联排版 (`cooltv_plus.png` / `bilibili.png` / `formatDouyinBrandTextHtml`)**：重新生成斜体无留白 `cooltv_plus.png`（`953×238`）及修正 `b` 下半圆扁平度的无留白 `bilibili.png`（`962×384`）；`"很酷 TV+"` 走 `.inline-cooltv-logo` 内联渲染；`bilibili` 丸形菜单内自动在 Logo 与分类名间插入空格；`.tmdb-logo-stage` 与 `renderHeroLogoElement()` 所有尺寸下调 20%。
  3. **密码框虚拟焦点态防 Esc 劫持 (`setModalFocus` / `#inputUnlockPwd`)**：将 `#inputUnlockPwd` 改为 `type="text"` 配合 `-webkit-text-security: disc;`，在 `setModalFocus()` 中对 `#inputUnlockPwd` 仅施加 `.modal-focused` 视觉高亮而不触发原生 DOM `.focus()`，由全局 `keydown` 接管密码字符输入、退格与 `Esc` 退出，彻底根除 `WKWebView` 原生密码框吞噬/破坏 `Esc` 键的问题。
  4. **设置页焦点矩阵与 DOM 视觉顺序严格对齐 (`getSettingsModalRows`)**：将 `[btnToggleSystemVpn]` 与 `[btnClearWatchHistory]` 移至第 5、6 行（紧随账号登录行之后），使最后一项 `btnToggleVolumeNorm` 按下键直达底部 `取消 / 保存并扫描`。
  5. **返回键（Esc）侧边栏兜底与正式播放页面无缝往返 (`keydown` / `enterNowPlayingFullscreen` / `executeItemAction`)**：当 `col === 0 && row !== 7` 时按 `Esc` 移动焦点至 `#navNowPlayingItem`；当焦点位于“正在观看”时按 `Esc` 调用 `enterNowPlayingFullscreen()` 丝滑切入正式播放页面（锚定 `#navNowPlayingItem` 焦点并复用实时播放进度），当显示为“现在观看”时按 `Esc` 不作任何反应。

### 14. 副标题分隔符与 Logo 透明度规范、同剧集观看历史合并、流媒体大标题分类化、全屏独占与 Esc 锁死及 B 站登录态推荐修复（Round 22 重点复盘 Task 1–11）
- **历史异常表现与优化诉求**：
  1. **副标题空格缺失与 Logo 笔画/透明度细节（Task 1, 2, 4, 10）**：播放进度条上方及主界面副标题中，集数（`第 X 集`）与 `很酷 TV+` 之间缺少空格，且存在未统一为左右各一空格的 `·` 号；`很酷 TV+` Logo 文字笔画偏细需轻微加粗；当 `bilibili` 与 `很酷 TV+` Logo 位于丸形菜单或副标题时需增加透明度以匹配整体层级（大标题内及丸形菜单选中时不透明）；TMDB 剧集 Logo 需在上一轮基础上继续缩小 20%。
  2. **同剧集多集在“正在观看/现在观看”重复堆积（Task 3）**：观看同一部剧集的不同集时，“正在观看/现在观看”会生成多条同剧集历史卡片，需永远合并为一条记录并以最新集数与播放进度为准。
  3. **流媒体大标题展示方式调整（Task 5）**：流媒体（Row 3）大标题（`#heroTitle`）不再跑马灯滚动当前视频标题，改为固定显示当前丸形菜单对应的分类名称（正确渲染内联 Logo 与 `.douyin-font` 字体），并将默认状态标题由 `抖音` 改为 `抖音精选`；卡片下方标题与正式播放页面标题仍显示视频标题与作者。
  4. **窗口模式最大化隐藏鼠标指针与 Esc 触发全屏切换（Task 6, 7）**：旧版 `isAppInFullscreenMode()` 误将窗口宽高等于屏幕宽高（窗口模式最大化）判定为全屏，且 `.media-item` 硬编码了 `cursor: none !important`；同时 macOS `WKWebView` 默认会将 `Esc` 键传递给 Cocoa `cancelOperation:` 导致退出全屏独占模式。
  5. **启动焦点、禁止自动播放未看新内容与 IPTV 上下换台反转（Task 8, 9）**：每次启动 APP 时焦点需自动落在“现在观看/正在观看”上，仅允许根据历史记录自动恢复播放上次退出的内容，严禁自动播放未观看过的全新内容；IPTV 正式播放页上下键换台需调换为“下键上一频道、上键下一频道”。
  6. **B 站已登录但推荐与热门仍为游客模式内容（Task 11）**：用户在设置中确认 B 站已登录且设置内打开的 B 站首页为个性化内容，但“bilibili 热门”和“bilibili 推荐”返回的内容与个人相关性差，形同游客模式。
- **底层根因与最终解决方案**：
  1. **中圆点标准间距归一化与内联 Logo 透明度层级 (`normalizeMiddleDotSpacing` / `formatDouyinBrandTextHtml` / `cooltv_plus.png`)**：新增 `normalizeMiddleDotSpacing()` 将所有包含 `·` 的副标题统一格式化为 `' · '`（修复 `buildMediaCardHTML` 中 `suffix = ' · ' + sourceLabel`），并在 `formatDouyinBrandTextHtml()` 中支持将副标题与丸形菜单内的 `很酷 TV+` 渲染为 `.inline-cooltv-logo`；对 `cooltv_plus.png` 与 `很酷 TV+.PNG` 进行径向抗锯齿笔画微加粗（中位笔画宽度提升约 6.5%）；通过 CSS 为 `.season-pill`（`0.75`）、`.sub-title` / `.iptv-hud-sub`（`0.78`）、`.card-meta`（`0.55`）中的 `.inline-bili-logo` 和 `.inline-cooltv-logo` 设置专属透明度，而在 `.main-title`、`.iptv-hud-title` 与 `.season-pill.focused` 中保持 `opacity: 1`；TMDB 剧集 Logo 尺寸再乘以 `0.8`（缩小 20%）。
  2. **剧集规范化唯一键合并引擎 (`getCanonicalSeriesKeyForMeta` / `recordWatchHistoryItem` / `syncNowWatchingRowData`)**：构建 `getCanonicalSeriesKeyForMeta()`（优先提取 `seriesKey` / TMDB ID / 剥离季集后缀的纯净剧名 + 所属行号），在写入、读取 `localStorage` 及渲染 Row 7 时对同剧集不同集强制合并去重，卡片标题固定为剧集主名称，副标题与进度条实时更新为最新观看的季集与进度百分比。
  3. **流媒体大标题绑定当前丸形菜单 (`getCurrentStreamingHeroTitle` / `updateStreamingHeroHeader`)**：将 `#navDouyinItem` 默认标题设为 `抖音精选`，在流媒体板块下由 `updateStreamingHeroHeader()` 根据当前选中的一级/二级丸形菜单实时渲染 `#heroTitle`（支持 `.douyin-font` 与 `.inline-bili-logo`），同时保持 `.card-title` / `.card-meta` 及正式播放页 `#iptvHudTitle` 显示单集视频标题和作者。
  4. **原生窗口全屏独占检测与全局 Esc 捕获锁死 (`is_window_fullscreen` / `set_window_fullscreen_state` / `syncAppCursorVisibility`)**：移除 `.media-item` 的 `cursor: none !important` 及前端基于窗口尺寸的伪全屏推断，仅依据 Tauri 原生 `isFullscreen()` / HTML5 Fullscreen API 判定全屏独占模式；在 `window` 捕获阶段拦截 `Escape` 的 `keydown`/`keyup` 并调用 `preventDefault()` + 主动恢复因系统级快捷键异常改动的窗口全屏状态，彻底禁止 `Esc` 切换窗口与全屏独占模式。
  5. **启动聚焦“现在观看”与历史续播守卫 + IPTV 换台方向修正 (`initApp` / `setFocus` / `switchFullScreenVideo`)**：`initApp()` 启动时固定聚焦 `#navNowPlayingItem`（`row=7, col=0`），仅当 `watchHistoryList.length > 0` 时允许自动续播上次观看的历史记录，彻底移除未观看新内容的自动播放兜底；正式播放页下当 `isCurrentStreamIptv()` 为真时，将 `ArrowDown` 映射为 `switchFullScreenVideo(-1)`（上一频道）、`ArrowUp` 映射为 `switchFullScreenVideo(1)`（下一频道），并支持从“正在观看”进入 IPTV 全屏时跨行定位 `ROW_DATA[2]` 频道列表。
  6. **WKHTTPCookieStore 原生 HttpOnly Cookie 同步 + WBI 签名与同源 WebView 抓取 (`sync_bili_cookies_from_webview` / `sign_bili_wbi_query` / `window.__FC_FETCH_BILI__`)**：B 站核心登录态 Cookie `SESSDATA` 带有 `HttpOnly` 属性，前端 `document.cookie` 无法读取，导致旧版传给后端 `curl` 的 Cookie 缺失 `SESSDATA` 而退化为游客模式，且旧版 `bili_rcmd` 失败时直接回退到了通用热门接口。后端新增 `sync_bili_cookies_from_webview()` 通过 Tauri `win.cookies_for_url("https://www.bilibili.com/")` 直接从 macOS `WKHTTPCookieStore` 提取包含 `SESSDATA`、`bili_jct`、`DedeUserID` 在内的完整 Cookie 落盘缓存，结合纯 Rust 实现的 WBI 实时混洗签名（`md5_hex` + `extract_wbi_mixin_key` 生成 `w_rid` & `wts`）与 `bilibili_scraper` WebView 内带 `credentials: 'include'` 的同源请求及首页 `rcmd` 拦截缓存，确保“bilibili 推荐”与“bilibili 热门”100% 携带真实登录态返回高相关度个性化内容。

### 15. 亮度步长微调、流媒体 16 字节对齐防错位、流媒体点赞/收藏与只看作者菜单、多级设置重构、剧集/电影分栏及观看历史流媒体源标题去重（Round 23 重点复盘 Task 1–12）
- **历史异常表现与优化诉求**：
  1. **亮度步长、流媒体上下切流方向与边看边补滚动加载（Task 1, 4, 5）**：电视亮度调节步长需从 `20` 改为 `10`；流媒体正式播放页面上下键切流方向需与有线电视（IPTV）统一为“下键上一个视频、上键下一个视频”；流媒体列表只要播放一个视频，就在列表结尾自动补足追加一张新视频卡片实现无感滚动加载。
  2. **部分竖屏/定制编码流媒体视频画面分块错位与静止（Task 3）**：部分 9:16 竖屏流媒体视频在经过 `scale='min(1920,iw)':'min(1080,ih)'` 缩放后宽度变为 `606`（非 16 的整数倍，色度平面宽度为奇数 `303`），触发 macOS `h264_videotoolbox` 硬件编码器行跨距（Row Stride）错位；同时输入端 `-fflags +discardcorrupt` 在网络抖动时会丢弃参考帧导致后续 P 帧局部静止。
  3. **流媒体正式播放页点赞/收藏互动与 F16“只看作者”菜单（Task 6, 7）**：流媒体正式播放页需支持长按 `Return` 或单击 `F17` 触发收藏、双击 `Return` 触发点赞、单击 `Return` 暂停/继续播放，并在标题右侧居右显示状态指示器（抖音为桃心+五角星，bilibili 为大拇指+五角星；正常白色、点赞品红色、收藏黄色）；按下 `F16` 需唤出流媒体专用选项菜单（提供带对勾 SVG 指示器的“只看作者”模式，开启后上下键切换该作者作品）。
  4. **视频播完自动切入下一个视频后，返回上一个视频黑屏且进度卡在中段（Task 8）**：当一个视频播放完毕自动进入下一个视频后，该视频在历史记录中仍残留接近末尾的进度，导致用户按返回/切回上一个视频时跳到片尾附近黑屏卡死，无法从头播放。
  5. **多级全屏设置页面重构与很酷 TV+ 剧集/电影分栏（Task 9, 10）**：设置页需重构为三级全屏架构（一级分类入口 -> 二级设置项列表 -> 三级填写页，非填写性选项直接在二级页面右侧显示当前状态名）；“很酷 TV+”需区分“剧集”和“电影”两个二级丸形菜单（默认选中“剧集”），大标题对应显示 `"很酷 TV+ 剧集"` 或 `"很酷 TV+ 电影"`，且调整“很酷 TV+”内联图标尺寸使其与后面的“剧集”二字视觉大小完全一致而不过大；设置页分别提供独立的“剧集资源位置”和“电影资源位置”。
  6. **取消“正在观看/现在观看”的“全部”分类，流媒体历史项大标题显示数据源且消除“抖音 抖音精选”重复前缀（Task 11, 12）**：“正在观看/现在观看”移除“全部”丸形菜单，保留“很酷 TV+”、“有线电视”、“流媒体”（及解锁后的“限制级”）；流媒体历史项大标题显示对应数据源分类名（如 `"抖音精选"`），并修复因拼接导致的 `"抖音 抖音精选"` 重复品牌词问题。
- **底层根因与最终解决方案**：
  1. **16 像素宏块对齐与 H.264 源优先 (`build_ffmpeg_args` / `probe_douyin_candidates` / `isHevcOrByteVCEntry`)**：移除 FFmpeg 输入端 `+discardcorrupt`，将流媒体与本地视频缩放滤镜统一升级为 `scale='min(1920,iw)':'min(1920,ih)':force_original_aspect_ratio=decrease,scale=trunc((iw+8)/16)*16:trunc((ih+8)/16)*16,setsar=1,fps=30,format=yuv420p`，确保任意横竖屏视频宽高严格对齐 16 的整数倍；在抖音嗅探与 `ffprobe` 预检中严格将标准 H.264 源排在首位。
  2. **同源 WebView 点赞/收藏与作者作品抓取 (`interact_stream_item` / `get_stream_author_videos` / `isCurrentStreamBilibili`)**：在 `douyin_scraper` 与 `bilibili_scraper` 内注入同源互动与作者投稿抓取桥接函数；前端通过 `isCurrentStreamBilibili()` 识别 `bili:` / `bilibili:` / `bilibili.com` 协议切换大拇指与桃心 SVG，并在 `toggleStreamAuthorOnlyMode()` 中解析后端返回的 `DouyinItem` 元组列表供上下键无缝切换。
  3. **完播进度归零与上下键切流强制从零起播 (`resetWatchHistoryProgressForUrl` / `forceStartFromBeginningOnce`)**：在 `ended` 事件及 `triggerDouyinAutoNextIfNeeded()` / `playNextLocalSeriesEpisodeIfAvailable()` 切入下一视频前调用 `resetWatchHistoryProgressForUrl()` 将刚播完视频的历史进度归零，并在 `switchFullScreenVideo()` 中设置 `forceStartFromBeginningOnce = true`，确保切回上一视频时 100% 从 0 秒正常起播。
  4. **三级全屏设置页状态机与剧集/电影双库刮削 (`openSettingsLevel2` / `openSettingsLevel3` / `scrapeSingleCoolTvCategory`)**：构建三级设置视图与键盘左右键状态切换/进退栈；将本地影视库拆分为 `seriesPath`（`COOLTV_SERIES_ITEMS`）与 `moviePath`（`COOLTV_MOVIE_ITEMS`）；将 `.inline-cooltv-logo` 精确调校为 `height: 0.97em; vertical-align: -0.11em;` 以对齐“剧集/电影”汉字字高；通过 `getCurrentDouyinSubTitle()` 直接返回 `found.name` 并配合 `sanitizeStreamSourceTitle()` 彻底消除 `"抖音 抖音精选"` 等重复品牌前缀。

### 16. 【重点故障专项复盘】流媒体播放“画面切块停滞”深度根因剖析与根治方案、设置页未选中去外框、圆角桃心/五角星、流媒体选项左遮罩清理、正式播放页柔化阴影及“只看作者”上下键切流修复（Round 24 重点复盘 Task 1–6）

#### (一) 重点故障专项深度复盘：流媒体播放严重解码问题与“画面切块停滞”（Task 4）
- **故障现象描述**：
  在播放流媒体（抖音精选、bilibili 推荐、bilibili 热门）视频时，画面频繁出现严重的局部宏块/分片停滞现象（即画面中某几个矩形分块或下半部分 Slice 静止卡死，而其余区域仍在更新，导致画面割裂、马赛克错位与间歇停顿），且此前在 Round 17、Round 19、Round 23 中多次尝试调整缩放参数与缓冲均未能根治。
- **全链路二进制抓包与 `ffprobe` / `trace_headers` 深度根因诊断（共定位 5 个相互叠加的底层根因）**：
  1. **【核心根因 1：抖音 `br.VideoExtra` 字段 JSON 键名误匹配导致 100% H.264 源被误判为 ByteVC1/HEVC 并丢弃】**
     - 在 `src-tauri/src/lib.rs` 的 `ensure_douyin_scraper` 注入脚本中，Round 23 的 `isHevcOrByteVCEntry(br)` 编写了如下判断：
       ```javascript
       const g = String(br.gear_name || '') + ' ' + String(br.codec_type || '') + ' ' + String(br.VideoExtra || '');
       if (/h265|hevc|bytevc|bvc|av01/i.test(g)) return true;
       ```
     - **致命陷阱**：抖音网页版接口返回的 `v.bit_rate` 数组中，**每一个 H.264 档位（如 `normal_1080_0`、`normal_720_0`）的 `br.VideoExtra` 都是一个序列化 JSON 字符串，其中固定包含键值对 `"is_bytevc1":0`**！
     - 正则表达式 `/bytevc/i` 直接匹配到了键名 `"is_bytevc1":0` 中的子串 `"bytevc"`，导致 **`isHevcOrByteVCEntry(br)` 对 100% 的 H.264 档位全部返回了 `true`**！
     - 进而导致 `pickBestPlayUrl(v)` 第一轮筛选 H.264 的循环（`if (!isHevcOrByteVCEntry(br))`）跳过了所有真正的 H.264 源，直接落入第二轮 HEVC 兜底循环，并按码率/帧率排序取出了排在最前面的 **ByteVC1 (`hevc`, 60fps Open-GOP)** 源的 3 个 CDN 镜像地址，再被 `candidates.slice(0, 3)` 截断（连排在后面的 H.264 `download_addr` 也被截断丢弃）。
     - 随后后端 `probe_douyin_candidates` 探测这 3 个候选地址均为 `codec_name=hevc`，触发 `fallback_hevc` 兜底放行，最终把**抖音私有定制的 Open-GOP ByteVC1 (`hevc`) 视频流**送进了标准 FFmpeg 的软件 `hevc` 解码器！由于抖音 ByteVC1 采用了非标准 CRA/RASL 开放式 GOP 参考帧结构与私有 CTU Tile 预测，标准 FFmpeg `hevc` 解码器在跨帧参考时出现局部 CTU 块参考丢失，直接输出了局部冻结的半解码画面（即“画面切块停滞”）。
  2. **【核心根因 2：`scale=trunc((iw+8)/16)*16` 将标准 `1080` 分辨率强行拉伸为非标 `1088`（`frame_cropping_flag=0`）】**
     - 通过对 `/tmp/cooltv_hls/seg_*.ts` 执行 `ffmpeg -bsf:v trace_headers` 比特流分析发现，Round 23 引入的 `scale=trunc((iw+8)/16)*16:trunc((ih+8)/16)*16` 由于 `1080 / 16 = 67.5`，将 `1920x1080` 强行拉伸成了 `width=1920, height=1088`（竖屏 `1080x1920` 被拉伸成 `1088x1920`），且输出的 H.264 SPS 中 `frame_cropping_flag = 0`。这不仅破坏了原生像素行对齐，还导致 `h264_videotoolbox` 与 WebKit 硬件解码器在非标 `1088` 纹理高度下发生 Tile 合成错位。
  3. **【核心根因 3：FFmpeg 9.0.2 自动透传源流私有 `User Data Unregistered SEI (nal_unit_type=6)` + 多 Slice / 硬件编码器争用】**
     - 通过 `ffprobe -show_frames` 与 `trace_headers` 实测发现，FFmpeg 9.0.2 在解码抖音/B站源视频时，会将输入 `AVFrame` 中携带的私有 `H.26[45] User Data Unregistered SEI message`（UUID `47564a...` 等编码器私有帧级元数据）原样注入到输出的 H.264 比特流（`nal_unit_type = 6`）中，干扰前端 `hls.js` 的 `TSDemuxer` 与 WebKit MSE 解码器。
     - 同时，`h264_videotoolbox -realtime 1` 编码出的帧采用 `slice_type = 2 / 0`（允许单帧多 Slice 切分），且与前台/后台 WebView 共享同一个 Apple Silicon VideoToolbox 硬件会话池。
  4. **【核心根因 4：隐藏抓取窗口（`douyin_scraper` / `bilibili_scraper`）在后台持续运行硬件视频解码与 60fps 定时器】**
     - `douyin_scraper` 与 `bilibili_scraper` 两个隐藏 `WKWebView` 此前仅对页面中的 `<video>` 执行了静音（`el.muted = true; el.volume = 0`），**从未调用 `el.pause()` 或卸载 `src`（且未拦截 `HTMLMediaElement.prototype.play`）**！
     - 由于隐藏窗口注入了 `document.visibilityState = 'visible'` 及 `16ms` 的 `requestAnimationFrame` 定时器，抖音网页自身的 `xgplayer` 播放器在后台隐藏窗口内持续全速下载并硬件解码 ByteVC1 视频，与主窗口 `main` 共同争抢 macOS 单例 `com.apple.WebKit.GPU` 进程和 VideoToolbox 硬件解码通道！
  5. **【核心根因 5：HTTP 输入端 `-reconnect_streamed 1` 破坏 MP4 样本对齐 + HLS 输出端缺失 `+temp_file` 原子写入】**
     - 流媒体源本质上是带 `Content-Length` 的渐进式 MP4 文件而非无限直播流。开启 `-reconnect_streamed 1` 后，当 CDN 分块连接闪断重连时，FFmpeg 不会重置 MP4 `stco/stsz` 样本偏移表，直接将错位字节喂给解码器造成半帧花屏停滞。
     - 同时，输出端 `-hls_flags` 仅设置了 `independent_segments` 而未开启 `+temp_file`，导致 FFmpeg 在原地直接写入 `.ts` 与 `.m3u8` 文件；当 `hls.js` 高频轮询本地 `127.0.0.1:8765` 时，`rouille` 会偶发读取到尚未刷盘完毕的半截 `.ts` 切片（丢失帧下半部分 Slice 的 TS 包），直接触发画面下半截切块停滞。
- **系统性工程根治方案**：
  1. **精准修复抖音 H.264 档位识别与 `MediaCapabilities` 拦截 (`ensure_douyin_scraper` / `probe_douyin_candidates`)**：
     - 修正 `isHevcOrByteVCEntry(br)` 与 `isByteVC2Entry(br)`，对 `br.VideoExtra` 严格匹配 `/"is_bytevc1"\s*:\s*[12]|"is_h265"\s*:\s*[12]/i`，彻底杜绝 `"is_bytevc1":0` 被误判为 HEVC；
     - 额外 Hook `window.MediaCapabilities.prototype.decodingInfo`，对 `hev1|hvc1|hevc|bytevc|av01|vp9` 一律返回 `{ supported: false }`；
     - 在 `probe_douyin_candidates` 中移除 `fallback_hevc` 兜底，**100% 严格只放行经 `ffprobe` 实测 `codec_name == "h264"` 的标准 H.264 视频源**。
  2. **彻底阻断后台隐藏 WebView 视频解码与 GPU 争抢**：
     - 在 `douyin_scraper` 中拦截非登录态下的 `HTMLMediaElement.prototype.play()` 直接暂停返回，并在 `douyin_scraper` 与 `bilibili_scraper` 的巡检定时器中强制执行 `el.pause(); el.removeAttribute('src'); el.load();`，同时在首屏加载完成后将后台 `requestAnimationFrame` 节流至 `48ms`，实现后台隐藏窗口 **0 视频解码、0 GPU 纹理占用**。
  3. **重构流媒体 FFmpeg 单 Slice 整帧编码与原子 HLS 切片管线 (`build_ffmpeg_args` / `start_ffmpeg`)**：
     - 移除流媒体分支的 `-reconnect_streamed 1`；
     - 将缩放滤镜修正为 `scale='min(1920,iw)':'min(1920,ih)':force_original_aspect_ratio=decrease,scale=trunc(iw/4)*4:trunc(ih/4)*4,setsar=1,fps=30,format=yuv420p`，保持 `1920x1080` 与 `1080x1920` 原生尺寸不变且确保 `yuv420p` 亮度/色度平面严格偶数对齐；
     - 流媒体分支采用 `libx264 -preset veryfast -profile:v main -level 4.0 -pix_fmt yuv420p -bf 0 -g 30 -keyint_min 30 -sc_threshold 0 -flags +cgop -x264-params sliced-threads=0:ref=1:bframes=0:force-cfr=1`：通过 **`sliced-threads=0`** 强制每一帧编码为单一完整整帧 Slice（`slice_type = 7 / 5`，`first_mb_in_slice = 0`），从编码结构上彻底消除单帧内局部 Slice 停滞的可能，并规避 VideoToolbox 硬件争用；
     - 注入比特流滤镜 `-bsf:v filter_units=remove_types=6` 彻底剥离源流透传的私有未注册 SEI（`nal_unit_type=6`）；
     - 全局启用 `-hls_flags independent_segments+temp_file` 保障 `.ts` 与 `.m3u8` 100% 先写临时文件再原子重命名发布，并在 `start_ffmpeg` 中确保流媒体至少就绪 2 个完整分片（`_0001.ts`）或转码完成后再通知前端 `hls.js` 起播。

#### (二) 其余 5 项界面与交互优化复盘（Task 1, 2, 3, 5, 6）
1. **设置页面未选中项隐藏按钮外框（Task 1）**：将 `.settings-nav-row-btn` 默认未聚焦状态调整为 `border: 2px solid transparent; background: transparent; box-shadow: none;`，仅在聚焦选中（`.modal-focused` / `:focus`）时呈现纯白高亮底框与阴影。
2. **流媒体点赞桃心与收藏五角星全圆角化设计（Task 2）**：重写 `#fsStreamLikeSvg`（`DOUYIN_HEART_SVG`）底部尖角为三次贝塞尔圆弧过渡（`c-.76.69-1.93.69-2.69-.01`），重写 `#fsStreamCollectSvg`（`STREAM_STAR_SVG`）五个外角为平滑圆弧顶点并配合 `stroke-linejoin="round" stroke-linecap="round"`，彻底消除尖锐视觉感。
3. **清理“流媒体选项”与流媒体二级菜单左侧跑马灯渐变遮罩残余（Task 3）**：移除 `#fsStreamMenuViewport` 与 `.season-pills-scroll-viewport` 上遗留的 `-webkit-mask-image: linear-gradient(to right, transparent 0px, black ...)` 遮罩及负边距内缩，消除左侧阴影裁切感。
4. **正式播放页面文字与按钮元素阴影全面柔化（Task 5）**：大幅提升 `.iptv-hud-title`、`.iptv-hud-sub`、`.fs-time-label`、`.fs-ep-picker-title`、`.fs-ep-picker-season-meta`、`.fs-stream-ind-item`、`.tv-brightness-icon svg`、`.fs-ep-item-btn.focused`、`.fs-progress-fill` 及 `.fs-progress-thumb` 的阴影模糊半径（Blur Radius 提升至 `20px–34px`）并降低核心不透明度（`0.28–0.52`），采用双层柔和扩散阴影替代原先紧聚生硬的高浓度黑影。
5. **修复流媒体开启“只看作者”后无法用上下键切换作者视频的问题（Task 6）**：
   - **根因**：① 抖音网页端 `bdms` 签名拦截器挂载在 `XMLHttpRequest` 上，旧版 `__FC_FETCH_DOUYIN_AUTHOR__` 使用原生 `fetch` 请求 `/aweme/v1/web/aweme/post/` 缺少 `a_bogus` 签名导致返回空响应；② 旧版 `handleFsStreamMenuKey` 在“流媒体选项”菜单打开时按上下键仅关闭菜单而未触发切流；③ 旧版 `switchFullScreenVideo` 在 `streamAuthorOnlyIndex = 0` 时按下键（`step = -1`）计算出 `targetAuthorIdx = -1 < 0` 直接 `return` 退出。
   - **修复**：在 `douyin_scraper` 中改用 `XMLHttpRequest`（自动触发 `bdms` 生成 `a_bogus`）并增加 `/user/{sec_uid}` SSR 数据提取与关联视频三级兜底；在 `bilibili_scraper` 中增加 `x/space/wbi/arc/search` WBI 签名空间投稿检索兜底；在唤出 `#fsStreamMenuHud` 时即刻预抓取作者视频；在 `handleFsStreamMenuKey` 中按上下键自动收起菜单并立即执行 `switchFullScreenVideo(±1)`；在 `switchFullScreenVideo` 中采用环形取模索引 `((streamAuthorOnlyIndex + step) % curLen + curLen) % curLen` 并支持异步等待加载完成后自动切流。

### 17. 【重大故障事件与重点任务专项复盘】流媒体播放“画面切块卡住无法正常播放”终极根因剖析、bilibili 账户登录态视频流关联性全链路排查修复、APP 使用一段时间后无响应（旋转彩球死锁）深度根治、设置页焦点残影消除及流媒体收藏图标视觉等大调优（Round 25 重点复盘 Task 1–5）

#### (一) 【重大故障事件复盘】部分流媒体视频播放时画面切块卡住无法正常播放（Task 1）
- **故障升级背景**：
  尽管 Round 24 修复了 `isHevcOrByteVCEntry` 的正则误判并引入了 `libx264` 单 Slice 编码，但在实际运行中，部分抖音与 bilibili 流媒体视频在起播或播放数秒后，依然会出现**画面切块（局部 Tile 撕裂/错位）并卡住无法正常播放**的严重故障，而本地影视（Row 1）与电视直播（Row 2）始终正常。
- **对 `/tmp/cooltv_hls/seg_*.ts` 生产现场切片的 `ffprobe` 逐包取证与 5 大真实根因**：
  1. **【决定性根因 1：MPEG-TS 默认 `muxdelay=0.7s` 造成 `1.423s` 首包时间轴空洞与音画起点错位，触发 `hls.js` `+0.1s` P 帧微跳（Nudge-Onto-P-Frame）】**
     - 对故障现场遗留切片 `/tmp/cooltv_hls/seg_1792417352478_26_0_0000.ts` 执行 `ffprobe -show_streams -show_packets` 实测发现：
       - 视频流 `start_time = 1.423222s`（首个 IDR 关键帧 `pts_time = 1.423222s`）；
       - 音频流 `start_time = 1.400000s`，且首片音频 `duration = 0.952s`（不足 `1.0s`，末尾停在 `2.352s`，与 `seg_0001.ts` 的 `2.423s` 之间存在 `71ms` 音频断层）。
     - **致命连锁反应**：当 `macVideo` 在 `currentTime = 0.000s` 挂载 MSE `SourceBuffer` 时，`[0.000s, 1.423s]` 属于未缓冲空洞（Buffer Hole）。而前端 `src/index.html` 中 `new Hls({...})` 配置了极其激进的 `maxBufferHole: 0.1, highBufferWatchdogPeriod: 2, nudgeOffset: 0.1`！
     - `hls.js` 的 `gap-controller` 检测到空洞大于 `0.1s`，立即执行 `macVideo.currentTime += 0.1s` 微跳（Nudge），且在分片交界处因音频 `71ms` 断层再次触发 `+0.1s` 微跳，**直接跨过了 `1.423s` / `2.423s` 处的 IDR 关键帧，将 `macVideo.currentTime` 强行 Seek 到了 `1.50s` 或 `2.50s` 的非关键帧（P 帧）上**！
     - 在 macOS `WKWebView` 的底层 `SourceBufferPrivateAVFObjC`（`VTDecompressionSession` + `AVSampleBufferDisplayLayer`）中，当 `currentTime` 被微跳到 P 帧时，硬件解码器丢失了前置 IDR 参考帧缓冲区，直接输出未刷新宏块的残缺帧（表现为画面上下或局部矩形切块撕裂），随后 `AVSampleBufferDisplayLayer` 因音画时钟失步挂起 `readyForMoreMediaData`（表现为画面彻底卡死无法继续播放）！
  2. **【决定性根因 2：竖屏（`9:16` / `3:4`）视频流在 `WKWebView` `<video width="1920" height="1080">` 负层级下的 `TiledCoreAnimationDrawingArea` 512×512 瓦片合成冻结】**
     - 为什么本地电影（Row 1）与 IPTV（Row 2）从不切块？因为它们全部是标准 `16:9` 横屏（`1920x1080` / `1280x720`），且全程固定 `object-fit: cover`。
     - 而抖音与部分 B 站视频是竖屏（如 `720x1280`、`1080x1920`、`1080x1440`），且 `macVideo` 被硬编码了 `macVideo.width = 1920; macVideo.height = 1080;`，置于 `z-index: -2`（上方覆盖 `#heroOverlay` 及侧边栏 `backdrop-filter: blur(50px)`），并在背景预览（`object-fit: cover`，将竖屏纵向拉伸至 `1920x3413` 裁剪）与正式播放（`object-fit: contain`）之间动态切换。
     - 在 macOS WebKit 的 `TiledCoreAnimationDrawingArea`（按 `512×512` 像素瓦片分块合成）中，当底层视频解码器输出的非 `16:9` 竖屏纹理尺寸与 `<video>` 元素的 `1920x1080` 固有属性发生几何冲突，且上方叠加 `backdrop-filter` 离屏高斯模糊采样时，WebKit 的脏矩形瓦片失效计算（Tile Invalidation）只更新了中心 `512×512` 瓦片，外围瓦片停留在上一帧，形成肉眼可见的“画面切块”。
  3. **【决定性根因 3：部分非 16 倍数视频源触发 H.264 SPS `frame_cropping_flag=1` 与 `libx264 veryfast` 高码率抖动】**
     - 实测 `seg_1792417352478_26_0_0000.ts` 的分辨率为 `576x764`（`764` 不是 16 宏块的整数倍，编码器被迫写入 `coded_height=768, frame_crop_bottom_offset=2`），在部分 Apple Silicon 机型的 `AVSampleBufferDisplayLayer` 缩放管线中极易引发底部宏块行错位。
  4. **【决定性根因 4：HTTP 输入端 `-multiple_requests 1` 在字节跳动/B站 CDN 上引发 HTTP Range 状态复原异常】**
     - 抖音 `douyinvod.com` 与 bilibili `mcdn.bilivideo.cn`（P2P 边缘节点）在复用同一 TCP 连接发送多段 Range 请求（`-multiple_requests 1`）或未过滤 `adapt_*` DASH 切片档位时，偶发返回截断的 MP4 chunk，导致 FFmpeg 输入流提前 EOF。
  5. **【决定性根因 5：首屏仅等待 `_0001.ts` 缓冲过浅 + 后台并发派生无锁 `ffprobe` 争抢 CPU/IO】**
     - `start_ffmpeg` 在检测到 `_0001.ts` 时即返回给前端播放，此时缓冲区仅约 `1.0s`；同时 `douyin_scraper` 每 35ms 弹出一条视频并无锁并发派生数十个 `ffprobe` 子进程，导致正在转码的 `ffmpeg` 进程发生瞬时 CPU 饥饿。
- **终极工程根治方案**：
  1. **强制归零 MPEG-TS 音视频起点 PTS 并消除音画微断层 (`build_ffmpeg_args`)**：
     - 视频滤镜链首部注入 `setpts=PTS-STARTPTS`；
     - 音频滤镜链注入 `aresample=async=1:first_pts=0,asetpts=PTS-STARTPTS`；
     - MPEG-TS 复用器显式添加 `-avoid_negative_ts make_zero -muxdelay 0 -muxpreload 0`；
     - 经 `ffprobe` 实测验证，新生成的 `seg_0000.ts` 视频与音频 `start_time` 严格等于 **`0.000000s`**，音频首片时长 `1.044s` 完全覆盖视频分片（`1.000s`），彻底消灭 `[0.0s, 1.423s]` 启动空洞与分片边界音频断层！
  2. **流媒体后端统一画布归一化为标准 `1280x720` (`16:9`, 80×45 整宏块, `frame_cropping_flag=0`) (`build_ffmpeg_args` & `macVideo`)**：
     - 对所有流媒体（抖音/bilibili）在 FFmpeg 滤镜链中统一执行：`setpts=PTS-STARTPTS,scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30,format=yuv420p`，并配合 `-c:v libx264 -preset ultrafast -tune zerolatency`；
     - 无论源视频是竖屏 `720x1280`、`576x764` 还是横屏 `1920x1080`，输出到 WebKit MSE 的 H.264 视频流 100% 恒定为标准 **`1280x720`（`16:9`，`80×45` 完整 `16×16` 宏块，`frame_cropping_flag=0`）**，与本地影视、IPTV 拥有完全一致的 `16:9` 几何拓扑；同时移除 `macVideo` 上冲突的硬编码 `width=1920 / height=1080` HTML 属性。
  3. **修正前端 `hls.js` 容差配置与首屏预缓冲深度 (`src/index.html` & `start_ffmpeg`)**：
     - 将 `hls.js` 参数调优为 `maxBufferHole: 0.5, highBufferWatchdogPeriod: 4, nudgeOffset: 1.0, nudgeMaxRetry: 3`：禁止 `0.1s` 微跳落入 P 帧，即便极端情况下需要跳过空洞也以 `1.0s`（整 GOP 关键帧步长）为单位对齐 IDR 帧；
     - 在 `start_ffmpeg` 中等待 `_0002.ts`（至少 3 个完整 1 秒分片）或 `#EXT-X-ENDLIST` 就绪后再挂载播放器。
  4. **优化抖音/B站源流选档与 HTTP 传输稳定性**：
     - 移除流媒体 HTTP 输入的 `-multiple_requests 1`；
     - 在 `pickBestPlayUrl` 中排除 `adapt_*` / `dash` 档位，优先选择标准 30fps `normal_720_0` / `normal_540_0` / `normal_1080_0` 渐进式 H.264 源；
     - 在 `resolve_bilibili_stream_candidates` 中对候选 URL 排序，优先使用官方高带宽 `upos-*` 主干 CDN，将不稳定的 `mcdn.bilivideo.cn` / `szbdyd.com` P2P 边缘节点降级至队尾。

#### (二) 【重点任务专项复盘】bilibili 视频流与登录账户仍不相关的全链路根因排查与修复（Task 4）
- **故障现象描述**：
  用户在设置页完成了 bilibili 账户登录，但流媒体板块展示的 bilibili 视频流仍然与用户的登录账户毫无关联（展示的始终是全站通用热门或游客态默认视频，而非登录账户网页端首页提取到的个性化推荐视频流）。
- **全链路代码审计定位的 5 个环环相扣的致命缺陷**：
  1. **【致命缺陷 1：每次播放任何视频时，`start_ffmpeg` 都会把 `/tmp/cooltv_hls/bili_cookie_cache.txt` 彻底删光！】**
     - Round 22 将 bilibili 的登录 Cookie 持久化保存在 `/tmp/cooltv_hls/bili_cookie_cache.txt` 中；
     - 然而在 `src-tauri/src/lib.rs` 的 `start_ffmpeg`（以及 `restart_app`）中，每次启动 FFmpeg 前都会遍历 `/tmp/cooltv_hls` 目录并删除其下的**所有文件**（`if p.is_file() { let _ = fs::remove_file(p); }`）！
     - 这意味着只要用户播放任意一个视频，持久化在磁盘上的 `bili_cookie_cache.txt` 就会被当场误删！
  2. **【致命缺陷 2：`bilibili_scraper` 每 1500ms 高频改写 `document.title = '__FC_BILI_COOKIE__:'` 覆盖并吞没了 `'__FC_BILI_FEED__:'` 消息】**
     - `bilibili_scraper` 通过修改 `document.title` 向 Rust 端的 `on_document_title_changed` 传递数据；
     - 但旧代码在一个 `1500ms` 的 `setInterval` 中无条件执行 `document.title = '__FC_BILI_COOKIE__:' + encodeURIComponent(ck) + '|||' + Date.now()`。当 `emitBiliNodes` 刚刚把 `document.title` 设为 `'__FC_BILI_FEED__:bili_rcmd|||...'` 时，极易在同一事件循环或 KVO 合并窗口内被 `'__FC_BILI_COOKIE__:'` 覆盖，导致 WebView 抓取到的登录态推荐流丢失！
  3. **【致命缺陷 3：隐藏 WebView (`visible: false`) 下 `<picture><source srcset>` 懒加载导致 `scrapeBiliHomepageDomCards()` 100% 丢弃首页 DOM 推荐卡片】**
     - 在 `bilibili_scraper`（`.visible(false)`）中，bilibili 首页 Vue/Pinia 渲染出的 `.bili-video-card` 采用 `<picture><source srcset="//i0.hdslb.com/..."><img src="" loading="lazy"></picture>` 结构。由于窗口处于隐藏态，WebKit 不会触发懒加载 `<img>` 的布局解码，因此 `imgEl.currentSrc` 和 `imgEl.src` 均为空字符串 `""`（真实封面地址保存在 `<source srcset>` 或 `data-src` 中）！
     - 而旧版 `scrapeBiliHomepageDomCards()` 编写了 `let pic = imgEl ? String(imgEl.currentSrc || imgEl.src || '').trim() : ''; if (!pic || !pic.startsWith('http')) return;`，导致 **bilibili 网页端已登录状态下渲染好的首页 DOM 推荐卡片被 100% 过滤丢弃**！
  4. **【致命缺陷 4：`get_douyin_feed` 仅等待 WebView 1.04 秒即回退到无 `SESSDATA` / 无 `w_webid` 的后端 `curl` 游客接口，且前端 `loadDouyinNativeFeed` 仅凭长度 `12 === 12` 永久锁死游客缓存】**
     - B 站登录核心凭证 `SESSDATA` 是 `HttpOnly` Cookie，`document.cookie` 无法读取它；只有 `bilibili_scraper` WebView 内部的同源 `fetch(..., { credentials: 'include' })` 才自动携带 `SESSDATA`，而后端的 `curl` 只能拿到非 HttpOnly Cookie，请求到的永远是游客视频流。
     - 旧版 `get_douyin_feed` 仅等待 `16 × 65ms = 1.04s`，一旦启动初期 `bilibili_scraper` 尚未加载完 `bilibili.com` 首页，Rust 后端立即调用 `fetch_bilibili_category_items`（`curl`）拉取 12 条游客视频塞入 `state.douyin_feed["bili_rcmd"]`。
     - 更致命的是：即使 2 秒后 `bilibili_scraper` 完成了登录态推荐流抓取并追加到 `state.douyin_feed["bili_rcmd"]` 队尾（第 13~24 条），前端 `loadDouyinNativeFeed` 执行 `cappedList = fullList.slice(0, 12)` 截出的前 12 条依然是那 12 条游客视频，且判断条件 `if (cappedList.length !== lastDouyinCount)` 因为 `12 === 12` 直接跳过重绘，将游客视频永久锁死在界面上！
  5. **【致命缺陷 5：前端 `BILI_CATEGORIES` 默认首项与 `activeBiliSubCategory` 被设为了非个性化的全站热门（`bili_popular`）而非个性化推荐（`bili_rcmd`）】**
     - 在 `src/index.html` 中，`BILI_CATEGORIES` 第一项和 `activeBiliSubCategory` 默认值是 `'bili_popular'`（调用 `/x/web-interface/popular` 全站热门排行榜，该接口本身就是全站公共热门，与个人账号推荐无关），而不是 `'bili_rcmd'`（`bilibili推荐`，对应网页版首页个性化推荐流）。
- **系统性工程修复方案**：
  1. **隔离持久化 Cookie 存储路径 (`BILI_COOKIE_CACHE_PATH`)**：将 Cookie 缓存文件移出 `/tmp/cooltv_hls`，独立存储在 `/tmp/cooltv_bili_cookie_cache.txt`，且 `start_ffmpeg` 仅清理 `.ts` / `.m3u8` / `.tmp` 切片文件，永不误删登录凭证。
  2. **重构 `bilibili_scraper` 同源网页提取引擎 (`ensure_bilibili_scraper`)**：
     - 新增 `scrapeBiliPiniaOrInitialState()` 直接读取 bilibili 首页内存中的 `window.__pinia.state.value` 与 `window.__INITIAL_STATE__` 推荐流数组；
     - 修复 `extractCardCoverFromDom(card)`，完整解析 `<picture><source srcset="...">`、`data-src`、`data-lazy-src` 与 `src`，确保隐藏 WebView 下 100% 提取出已登录首页的 DOM 推荐卡片；
     - 新增 `extractWWebIdFromDom()` 从首页 `<script id="__RENDER_DATA__">` 中提取 `access_id`（`w_webid`）注入 `/x/web-interface/wbi/index/top/feed/rcmd` 的 WBI 签名参数，并在 WebView 内原生支持全部 `bili_*` 分区接口（携带 `HttpOnly` 的 `SESSDATA`）；
     - 引入 `__FC_BILI_MSG_QUEUE__` 65ms 串行消息队列，且仅当 `document.cookie` 实际发生变化时才发送一次 `__FC_BILI_COOKIE__`，彻底杜绝消息互相覆盖。
  3. **已登录推荐流自动覆盖启动期游客缓存并实时通知前端重绘 (`bili_logged_in_cats` & `contentSig`)**：
     - `emitBiliNodes` 携带登录态标记 `isAuth = 1/0`（通过 `/x/web-interface/nav` 的 `isLogin` 及 `DedeUserID` / `bili_jct` 判定）；当首个 `isAuth == 1` 的已登录视频流到达 Rust 端时，立即清空此前可能残留的游客缓存并替换为已登录推荐流，同时触发主窗口 `window.__FC_ON_BILI_LOGIN_UPDATED__()`；
     - 前端 `loadDouyinNativeFeed` 新增内容签名比对（`contentSig = cappedList.map(it => it[3].split('@@')[0]).join('|')`），只要视频 BV 号列表发生变化，哪怕条数同为 12 条也立即重绘轨道；
     - 将 `BILI_CATEGORIES` 默认首项与 `activeBiliSubCategory` 修正为 `'bili_rcmd'`（`bilibili推荐`），确保用户切换到 bilibili 时默认直接呈现与登录账户强相关的网页端个性化推荐流。

#### (三) 【严重缺陷排查】应用程序使用一段时间后变为无响应状态（鼠标旋转彩球、Dock 栏提示无响应）（Task 5）
- **故障现象描述**：
  最近几个版本起，APP 运行一段时间后主界面彻底卡死，任何按键或鼠标点击均无响应，鼠标指针变为 macOS 旋转彩球（Spinning Beachball），Dock 栏显示“应用程序无响应”。
- **深度排查定位的 4 个核心致死缺陷**：
  1. **【首要致死根因：跨线程高频调用 `win.cookies_for_url()` 引发 macOS AppKit 主线程 `CFRunLoop` 与 `WKHTTPCookieStore` 重入死锁（Re-entrancy Deadlock）】**
     - Round 22 引入的 `sync_bili_cookies_from_webview` 在每次调用时都会对 3 个域名连续调用 3 次 `win.cookies_for_url(parsed)`；
     - 在 Tauri v2 (`wry`) 的 macOS 实现中，`cookies_for_url` 内部需要将读取任务派发到 macOS 主线程调用 WebKit 的异步 API `-[WKHTTPCookieStore getAllCookies:]`，并通过阻塞等待 `CFRunLoop` 回调返回结果；
     - 然而：① `bilibili_scraper` 每 `1500ms` 修改一次 `document.title`，在 **macOS 主线程** 的 WebKit KVO 回调 `on_document_title_changed` 中直接同步调用了 `sync_bili_cookies_from_webview`（主线程在 KVO 回调栈内重入等待 `WKHTTPCookieStore` 异步块完成）；② 与此同时，前端每 `450ms–600ms` 轮询一次 `get_douyin_feed`，在 Tokio 工作线程中并发调用 `sync_bili_cookies_from_webview` 争抢主线程派发锁与 `bili_cookie` Mutex！
     - 运行数分钟后，当主线程 `on_document_title_changed` KVO 回调与后台线程 `get_douyin_feed` 同时进入 `cookies_for_url` 时，主线程 `CFRunLoop` 与 WebKit IPC 队列发生经典重入死锁，主线程永久挂起，macOS WindowServer 立即判定应用无响应并弹出旋转彩球！
  2. **【次要致死根因：隐藏 WebView 每 `600ms` 执行 `el.removeAttribute('src'); el.load()` 触发网页播放器无限异常恢复风暴】**
     - Round 24 在 `douyin_scraper`（每 `600ms`）与 `bilibili_scraper`（每 `1500ms`）的定时器中对所有 `<video>` 执行了 `el.removeAttribute('src'); el.load()`；
     - 抖音的 `xgplayer` 与 B 站的 `bpx-player` 监听到 `<video>` 的 `emptied` / `error` 事件后，会立即启动异常恢复逻辑：重新创建 `MediaSource`、`SourceBuffer` 与 Blob URL 并重新赋值给 `video.src`；`600ms` 后又被定时器强行清空并 `load()`！如此每秒接近 2 次的“销毁-报错-重建 MSE”死循环在运行几分钟后耗尽了共享 `com.apple.WebKit.WebContent` / `GPU` 进程的句柄与内存。
  3. **【次要致死根因：前端 `loadDouyinNativeFeed` 无并发重入锁 + 后台 `__FC_LOAD_MORE__` 每次遍历 3000+ DOM 节点触发同步强制重排 + `ffprobe` 无上限并发派生】**
     - 前端 `douyinPollTimer` 每 `450ms` 调用一次 `loadDouyinNativeFeed`，但没有 `isLoadDouyinFeedInFlight` 互斥锁；当 `get_douyin_feed` 内部等待 `1.2s–2.5s` 时，前端每秒堆积多个未决 IPC 请求，且即使首屏 12 张卡片早已加载完毕，`douyinPollTimer` 也永不停止；
     - `douyin_scraper` 的 `__FC_LOAD_MORE__` 每次执行 `document.querySelectorAll('div, main, section, ul')` 遍历数千个节点读取 `el.scrollHeight > el.clientHeight`，触发极其昂贵的 WebKit 同步强制布局重排（Forced Synchronous Layout）；
     - `douyin_scraper` 的 `__FC_ITEM__` 回调无锁并发 `spawn_blocking` 派生数十个 `ffprobe` 子进程，耗尽 Tokio 阻塞线程池。
  4. **【次要致死根因：`is_window_fullscreen` 同步命令在主线程高频调用 & `ffmpeg` 未关闭 `stdin`】**
     - `is_window_fullscreen` 未声明为 `async`，每次调用都在主线程同步阻塞查询窗口状态；且 `start_ffmpeg` 未对子进程设置 `Stdio::null()` 和 `-nostdin`，在持有 `ffmpeg_process` 锁期间调用阻塞式 `child.wait()`。
- **系统性工程根治方案**：
  1. **彻底移除 `win.cookies_for_url()` 调用**：在 `sync_bili_cookies_from_webview`、`on_document_title_changed`、`on_window_event` 与 `get_douyin_feed` 中彻底移除会导致主线程 `CFRunLoop` 死锁的 `win.cookies_for_url()`，改为纯内存/磁盘轻量读取（而带 `HttpOnly` `SESSDATA` 的请求全部交由 `bilibili_scraper` WebView 内部同源 `fetch` 原生携带）。
  2. **移除 `el.removeAttribute('src'); el.load()` 并将后台静音巡检降频至 `3000ms–3500ms`**：依靠 `HTMLMediaElement.prototype.play` 拦截与 `el.pause()` 保持后台静音暂停，不再破坏播放器 DOM 状态引发无限重建循环。
  3. **为 `loadDouyinNativeFeed` 增加 `isLoadDouyinFeedInFlight` 重入锁与满额自动休眠机制**：当当前流媒体分类已满可见配额（12 张卡片）且稳定后，自动清除 `douyinPollTimer`，仅在切换分类、手动刷新或向右滚动加载更多时唤醒轮询；在后端引入 `douyin_probe_lock` 串行化 `ffprobe` 预检并限制后台缓存上限（18 条）；移除 `__FC_LOAD_MORE__` 中的全量 DOM 遍历。
  4. **将 `is_window_fullscreen` / `set_window_fullscreen_state` 升级为 `async` 命令，并为 `ffmpeg` 注入 `-nostdin` 与 `Stdio::null()`，先 `take()` 释放 Mutex 锁再执行 `kill() / wait()`**。

#### (四) 设置页面移动焦点动画残影修复与流媒体收藏图标视觉等大调优（Task 2 & Task 3）
1. **设置页面移动焦点残影滞留原因分析与修复（Task 2）**：
   - **成因诊断**：在 Round 24 将设置项未选中状态改为透明背景（`background: transparent; border: 2px solid transparent; box-shadow: none;`）后，`.settings-nav-row-btn` 仍然保留了 `transition: transform 0.22s ..., background-color 0.22s ease, color 0.22s ease, border-color 0.22s ease, box-shadow 0.22s ease;`。当用户按上下方向键将焦点从上一行移到下一行时，新聚焦行立即亮起纯白底框（`#ffffff`），而**失焦的上一行却在 `220ms` 内从纯白 `#ffffff` 向 `rgba(0,0,0,0)` 缓慢插值淡出（中途呈现半透明灰白色块与残留阴影）**；叠加在 `#settingsModal` 的 `backdrop-filter: blur(42px)` 毛玻璃合成层上，连续按上下键时屏幕上会同时滞留 2~3 个正在缓慢消退的半透明白框，造成明显的“残影滞留”与拖泥带水感。此外，`setModalFocus()` 在内容根本无需滚动的一级/二级页面上每次按键都强行触发 `target.scrollIntoView({ block: 'center', behavior: 'smooth' })`，引发异步滚动重绘抖动。
   - **修复方案**：
     - 将未聚焦的 `.settings-nav-row-btn` 显式设为 `background-color: rgba(255, 255, 255, 0); border: 2px solid rgba(255, 255, 255, 0); transform: translate3d(0, 0, 0) scale(1); backface-visibility: hidden;` 并配置 **`transition: transform 0.16s cubic-bezier(0.22, 1, 0.36, 1), background-color 0s, color 0s, border-color 0s, box-shadow 0s;`**，使失焦行在焦点移开的瞬间（`0ms`）立即恢复纯透明无阴影状态，实现 **0 残影滞留**；
     - 聚焦态 `.settings-nav-row-btn:focus, .settings-nav-row-btn.modal-focused` 独立配置丝滑的 GPU 硬件加速缩放与快速亮起过渡（`transform: translate3d(0, 0, 0) scale(1.015); transition: transform 0.2s cubic-bezier(0.22, 1, 0.36, 1), background-color 0.08s ease-out, box-shadow 0.14s ease-out;`）；
     - 在 `setModalFocus()` 中增加溢出判断 `if (settingsMask.scrollHeight > settingsMask.clientHeight + 24)`，未溢出视口时绝不触发多余的 `scrollIntoView`。
2. **轻微放大流媒体收藏五角星图标，确保与点赞图标视觉等大（Task 3）**：
   - **成因诊断**：虽然 `.fs-stream-ind-item svg` 将点赞图标与收藏图标的 SVG 容器均设为 `34px × 34px`，但在 `24×24` 的 `viewBox` 坐标系中，点赞桃心（宽 `20.9`、高 `18.2`，双圆弧饱满填充）和 B 站大拇指（宽 `22`、高 `20`）的视觉占空比很高，而旧版五角星路径仅占 `16.2 × 16.3` 单位且五角星具有内凹几何倾角，导致相同 `34px` 容器下五角星看起来明显偏小。
   - **修复方案**：将 `#fsStreamCollectInd svg` 的渲染尺寸轻微放大至 **`39px × 39px`**，并将 `STREAM_STAR_SVG` 的圆角五角星路径适度外扩至 `2.2..21.8` 满幅比例，使收藏五角星与点赞桃心/大拇指在视觉面积与外轮廓高度上完全一致。

---

### Round 26 (`v3.0.0`) 架构升级、视觉进化与疑难故障深度复盘（任务 1 ~ 17）

#### (一) 侧边栏与设置页选中胶囊平滑平移过渡 & 设置页顶部大标题冻结（Task 1 & Task 12）
1. **侧边栏导航项与设置页设置项焦点平滑平移（Task 1）**：
   - 在 `#sidebar` 内新增独立浮动高亮胶囊 `.sidebar-focus-pill#sidebarFocusPill`，在设置页各 `.settings-list-group` 内新增独立浮动白色胶囊 `.settings-focus-pill`；
   - 当焦点在侧边栏导航项（含顶部用户头像 `.avatar-mode`）或设置页各行（`.settings-nav-row-btn`）之间移动时，通过 `syncSidebarFocusPill(targetEl)` 与 `syncSettingsFocusPill(targetEl)` 实时读取目标元素的 `offsetTop` 与 `offsetHeight`，驱动 `transform: translate3d(0, y, 0) scale(...)` (`cubic-bezier(0.22, 1, 0.36, 1)`) 直接将选中样式的图形平滑平移到目标位置，视觉效果干净优雅。
2. **冻结设置页顶部大标题位置（Task 12）**：
   - 将 `#settingsModal` 设为 `overflow: hidden !important`，并将 `.fs-settings-page` 设为 `height: 100vh; display: flex; flex-direction: column; overflow: hidden;`；
   - 顶部 `.fs-page-header` 固定为 `flex-shrink: 0`，仅让下方的 `.settings-list-group` 作为独立滚动容器（`flex: 1; min-height: 0; overflow-y: auto;`），在 `setModalFocus()` 中仅滚动 `listGroup`，使顶部大标题始终冻结在原位不随选项滚动。

#### (二) 流媒体刷新旋转特效、卡片长标题跑马灯、副标题显示当前作者与“暂无视频内容”卡片重构（Task 2, 5, 8, 16）
1. **流媒体刷新指示器旋转特效（Task 2）**：
   - 补充 `@keyframes spinRefreshIcon`（`0% -> 360deg`）与 `triggerStreamRefreshSpin(minMs)`，并在 `updateDouyinRefreshBtnState()` 中同时联动顶部 `#streamRefreshIndicator` 与轨道末尾 `.refresh-card-wrapper`，只要触发刷新即自动旋转。
2. **流媒体卡片下方视频长标题跑马灯滚动（Task 5）**：
   - 新增 `.card-title.single-line-marquee` 渐变遮罩样式与 `syncStreamingCardTitleMarquee(focusedEl)`：当焦点移至流媒体卡片且卡片标题超出容器宽度时，自动调用双节点无闪烁跑马灯 `startMarqueePingPong()` 滚动展示完整标题；移开焦点时自动恢复静态省略态。
3. **“暂无视频内容”占位卡片全新毛玻璃 + 号设计（Task 8）**：
   - 参照流媒体刷新按钮卡片样式，新增 `.card-image-wrapper.empty-card-wrapper`（`background: rgba(255, 255, 255, 0.08); backdrop-filter: blur(25px);`）与居中 `+` 号 `PLUS_SVG_HTML`，统一应用于“很酷 TV+”、“正在观看/现在观看”及“限制级”无内容时的“暂无视频内容”占位卡片。
4. **流媒体页面副标题改为显示当前作者（Task 16）**：
   - 新增 `getCurrentStreamingHeroAuthorSub(preferredEl)`，在 `updateStreamingHeroHeader()` 及 `setFocus()`（`row === 3`）中将流媒体页面的副标题 `#heroSub` 由原先固定的 `"流媒体推荐视频"` 改为实时显示当前聚焦/选中流媒体视频的作者（如 `@作者名`）。

#### (三) 流媒体进入正式播放页自动暂停修复 & 任何入口进入正式播放不打断背景预览视频（Task 3 & Task 15）
1. **通过卡片进入流媒体正式播放页自动暂停的根因与修复（Task 3）**：
   - **根因诊断**：在主页面卡片上按下 `Enter` (`keydown`) 时会同步执行 `executeItemAction()` 并将 `isFullScreen` 置为 `true`。当用户在 `50~100ms` 后松开 `Enter` 键触发 `keyup` 时，`keyup` 监听器检测到 `isFullScreen && isCurrentStreamDouyin()` 为真，错误启动了 `240ms` 单击延时器 `fsStreamReturnClickTimer` 并调用 `toggleFullscreenPlayPause()`，导致进入正式播放页约 `0.24s` 后视频自动变成暂停状态！
   - **工程方案**：引入 `fsStreamReturnDownInFullscreen` 门禁变量，仅当已经处于 `isFullScreen === true` 状态下收到 `Enter` 的 `keydown` 事件时才置为 `true`，在 `executeItemAction()` 与 `exitFullscreenToMainView()` 中强制归零，并在 `keyup` 中校验 `if (!fsStreamReturnDownInFullscreen) return;`，彻底杜绝进入全屏松开回车键时误触发暂停。
2. **任何入口进入流媒体正式播放页面均不打断背景预览视频并从头播放的根因与修复（Task 15）**：
   - **根因诊断**：
     1. 聚焦流媒体卡片时调用的 `replenishOneStreamingCardOnPlay()` 递增了 `cache.visibleQuota` 却未同步更新 `cache.contentSig`（或与 `loadDouyinNativeFeed()` 的 `@@` 分割签名格式不一致），导致 `750ms` 后的下一次轮询误判 `sigChanged = true`（`wasPlaceholder = true`），从而清空重建 `#mainTrack` 并重跑 `setFocus()` 打断预览流；
     2. `executeItemAction()` 中判断 `isReusingActiveBgStream` 使用了严格字符串相等 `currentBgUrl === url`，当卡片/历史记录/后台流之间的 `@@` 附加元数据后缀或 `h264:` 前缀存在细微差异时判定为不同视频，进而调用 `destroyActiveHlsAndResetVideo()` 从头重启 FFmpeg；
     3. 当 `isReusingActiveBgStream` 为真时，原先无条件调用 `startVideoPlaybackWithFallback()` 会再次触发 `revealVideoWhenFrameReady()` 隐藏图层并重设播放。
   - **工程方案**：
     1. 统一使用 `computeStreamListSig(list)` 计算签名，并在 `loadDouyinNativeFeed()` 中识别前缀追加 `isAppendOnly = Boolean(prevSig && newSig.indexOf(prevSig) === 0)`，确保追加卡片永不触发整轨清空重建；
     2. 新增 `normalizeCanonicalMediaUrl()` 与 `isSameMediaStream(a, b)` 忽略前缀与 `@@` 元数据差异，并在 `enterNowPlayingFullscreen()`、`executeItemAction()` 与 `setFocus()` 中统一使用规范化比对；
     3. 当 `isReusingActiveBgStream` 成立且 `isVideoReady && !macVideo.paused` 时，直接保持 `macVideo` 连续播放并切换音画属性，实现从卡片、侧边栏“正在观看”、返回键或 `F18` 主页键进入正式播放页 100% 零打断无缝衔接。

#### (四) F16 菜单键与 F17 长按替代键严格解耦（Task 4）
- **根因诊断**：`handleF16ContextMenuKey()` 在非全屏分支（`!isFullScreen`）末尾错误调用了 `return triggerCurrentFocusLongPressAction();`，导致在主页聚焦“正在观看/现在观看”或“用户头像”时按 `F16` 键会误触发停止视频或解锁/锁定限制级空间。
- **修复方案**：从 `handleF16ContextMenuKey()` 中彻底移除 `triggerCurrentFocusLongPressAction()`。明确严格边界：**只有 `F17` 键可以替代长按 `Return` 键功能**；`F16` 键是纯粹的菜单键，在正式播放页用于唤出选集菜单/流媒体选项菜单/显隐进度条，在主页面卡片或丸形菜单区按下时用于唤出侧边栏并将焦点移回侧边栏当前导航项。

#### (五) 正式播放页标题阴影容器范围扩充 & 点赞与收藏符号间距优化（Task 6 & Task 7）
1. **正式播放页面标题阴影容器扩充（Task 6）**：
   - 保持 `.iptv-hud-title` 与 `.iptv-hud-sub` 原有恰到好处的 `text-shadow` 模糊半径不变，通过增加对称内边距与负外边距（`.iptv-hud-title`：`padding: 38px 42px; margin: -38px -42px -32px -42px;`，`.iptv-hud-sub`：`padding: 32px 36px; margin: -32px -36px;`）扩大阴影所在的容器与 `-webkit-mask-image` 离屏合成范围，彻底消除标题背景处不明显的半透明黑色矩形裁切边界。
2. **轻微左移流媒体点赞符号（Task 7）**：
   - 将 `.fs-stream-action-indicators` 的 `gap` 从 `20px` 增大至 `28px`，使左侧点赞符号轻微左移，增大点赞符号与收藏符号之间的视觉呼吸感。

#### (六) 抖音视频流加载卡住与抖音登录窗口图片 1 分钟不显示故障排查与根治（Task 9）
- **核心故障根因排查**：
  1. **【根因一：`window.fetch` 全局拦截器对所有响应无差别调用 `resp.clone()` 阻塞 WebKit HTTP/2 连接池 60 秒】**：
     - 在 `DOUYIN_INIT_SCRIPT` 中，旧版 `window.fetch` 拦截器对所有非媒体后缀请求（包括 `Request` 对象或 `URL` 对象导致 `reqUrl` 为空的图片/二进制分片请求）均执行了 `var clone = resp.clone(); clone.text()...`。在 macOS `WKWebView` 中，当抖音首页或登录窗口并发发起数十个图片/静态资源 `fetch` 时，`resp.clone()` 会迫使 WebKit `NetworkProcess` 将所有响应流缓存在双份内存管道中直至超时回收（约 60 秒），直接塞满同一主机的 HTTP/2 连接池，导致抖音登录窗口虽然加载出 HTML 骨架但页面上所有图片在 1 分钟内无法显示，同时也阻塞了后台推荐流 API 请求！
  2. **【根因二：`__FC_ITEM__` 对每条抖音视频串行启动 `ffprobe` 子进程探测耗时 30~60 秒】**：
     - 旧版 `pickBestPlayUrl(v)` 虽然已筛选 `is_bytevc1 === 0 && is_h265 === 0` 的 H.264 地址，但未标注 `h264:` 前缀；后端 `on_document_title_changed` 收到 30 条 `__FC_ITEM__` 时，在 `douyin_probe_lock` 互斥锁下逐条派生 `ffprobe` 联网探测 HTTP 视频流（每条耗时 1~3 秒），在代理或弱网下 30 条串行探测阻塞长达 45~60 秒，期间 `douyin_feed` 为空导致前端一直卡在加载占位卡片。
  3. **【根因三：`deepScan` 跳过了首屏 `_SSR_DATA` / `renderData` 水合数据】**：
     - 抖音精选页（`/discover`）首屏视频直接内嵌在 SSR 水合对象中，旧版 `deepScan` 将 `_SSR_DATA` 与 `renderData` 列入了跳过黑名单，若首屏 XHR 被延迟则无法提取任何卡片。
- **工程根治方案**：
  1. **精准收敛 `window.fetch` 克隆范围**：仅当请求 URL 明确匹配 `/aweme/v[12]/web/` 等 JSON 接口且响应头 `Content-Type` 包含 `json` 时才调用 `resp.clone()`，对图片、媒体及普通网页资源零克隆放行；同时在 `open_douyin_login` 中重置滚动位置并设置 `img.loading = 'eager'`，确保登录窗口图片秒开显示。
  2. **JS 层直接标记 `h264:` 并免除冗余 `ffprobe` 阻塞**：在 `pickBestPlayUrl(v)` 中对已通过 `bit_rate` 校验（`is_bytevc1 === 0 && is_h265 === 0`）或来自 `play_addr_264` / `play_addr_h264` 的地址直接冠以 `h264:` 前缀；后端收到带 `h264:` 前缀的条目后直接推入 `douyin_feed`，立即可供前端展示。
  3. **开放首屏 SSR 水合扫描**：从 `deepScan` 跳过列表中移除 `_SSR_DATA` / `renderData` / `initialState`，并新增 `scanInitialDouyinHydration()` 在页面就绪后 `650ms` 与 `1650ms` 主动扫描首屏水合数据。

#### (七) 左侧边栏高级材质升级与第三折叠方案“自动隐藏”（Task 10 & Task 14）
1. **左侧边栏高级多层材质与微光内边框（Task 14）**：
   - 将 `.sidebar` 升级为 `linear-gradient(165deg, rgba(38, 42, 54, 0.36) 0%, rgba(20, 22, 30, 0.24) 52%, rgba(14, 16, 22, 0.32) 100%)` 配合 `backdrop-filter: blur(48px) saturate(165%)`、外层 `1px solid rgba(255, 255, 255, 0.11)` 以及双层内高光阴影 `inset 0 1px 0.5px rgba(255, 255, 255, 0.20)`，使侧边栏通透细腻，与动态背景及预览视频自然融合。
2. **新增第三个侧边栏折叠方案“自动隐藏”（`autohide`，Task 10）**：
   - 在【设置 -> 个性化设置 -> 左侧边栏折叠方案】中，在原有“自动折叠”（`auto`）、“始终折叠”（`always`）基础上新增第三个方案 **“自动隐藏”**（`autohide`）；
   - “自动隐藏”沿用“始终折叠”逻辑（侧边栏平时永远只呈现 `68px` 折叠状态），但当用户将焦点移动到**第二个卡片起**（`col >= 2 && !isOnSeasonBar`），侧边栏附加 `.autohidden`（`transform: translate3d(-110px, 0, 0); opacity: 0;`）向左收起彻底不可见，同时 `#mainContent` 附加 `.sidebar-autohidden`（`left: 0; width: 100%;`），使第一个卡片的左侧渐变遮罩位置随之从侧边栏边缘平滑移至屏幕最左边缘；
   - 当用户按下**菜单键**（`F16`）或**返回键**（`Esc` / `Backspace`）时，立即重新唤出侧边栏并将焦点移动至侧边栏中的当前导航项（`col = 0`）。

#### (八) 本地持久化图片与影视文字缓存体系 + 手动清除/刷新覆盖（Task 11）
- **后端磁盘缓存与本地静态服务（`src-tauri/src/lib.rs`）**：
  - 在 `~/Library/Application Support/com.feelcool.tv/cache/` 下建立 `images/` 与 `meta/persistent_meta.json` 持久化目录；
  - 新增 `cache_remote_image(url, force)`、`save_persistent_metadata(json_data)`、`load_persistent_metadata()`、`clear_local_cache()` 四个 Tauri 命令，并在 `127.0.0.1:8765` HTTP 服务中新增 `/cache_img/<filename>` 路由（带 `Cache-Control: public, max-age=31536000`）。
- **前端自动缓存与设置项联动（`src/index.html`）**：
  - 对 TMDB 影视 Logo、剧集/电影封面图、IPTV 台标、用户头像等远程图片资源通过 `cacheRemoteImageLocal()` 自动下载缓存至本地磁盘并映射为 `http://127.0.0.1:8765/cache_img/...`；对 TMDB/豆瓣影视评分、影视介绍等文字信息通过 `scheduleSavePersistentMetadata()` 同步写入本地磁盘长期保存；
  - 在【设置 -> 媒体与账户设置】中新增 **“本地缓存数据”** 设置项（显示状态为 `清除图片与元数据缓存`，点击后手动清空全部本地图片与元数据缓存并反馈 `已清除图片与元数据缓存`）；
  - 当用户点击原有的 **“数据同步”** 功能（`reloadAllDataNow()`）时，自动覆盖更新本地缓存的图片与文字数据；只要用户不触发手动删除或数据同步刷新，本地缓存的图片与文字数据永久可用。

#### (九) 新增动态背景主题：“波浪”与“光影”（Task 13）
- 在【设置 -> 个性化设置】中新增设置项 **“背景主题”**（`bgStyle`），保留原动态背景主题并命名为 **“波浪”**（`wave`），新增动态背景主题命名为 **“光影”**（`light`）；
- 当选择 **“光影”** 时，`renderPspWaveFrame()` 使用 `globalCompositeOperation = 'screen'` 与多组无固定形状、随时间正余弦相位缓慢旋转变形与游走的径向渐变模糊光斑渲染出流动光影效果；所有 **“背景配色方案”**（随机、深海幽蓝、暮光星云、竹韵青灰、暖珀流沙、曜石银灰、绯樱夜雾）对“波浪”和“光影”均实时生效。

### 19. 【重点故障专项复盘】设置页少项垂直居中恢复、选中胶囊左边缘裁切修复、流媒体黑屏暂停无时长回归修复、光影主题降亮、侧边栏自动隐藏崩溃与联动左移修复及侧边栏边框柔化（Round 27 重点复盘 Task 1–6，v3.0.1）

#### (一) 设置页面少项垂直居中恢复 & 选中白色胶囊左侧被裁切修复（Task 1 & Task 2）
- **故障成因**：
  1. **设置页少项无法垂直居中（Task 1）**：Round 26 为实现顶部大标题冻结，将 `.fs-settings-page` 的高度从 `min-height: 0` + `margin: auto auto auto 0` 硬编码改为 `height: 100vh; margin: 0 auto 0 0;`。这导致 `.fs-settings-page` 始终撑满视口高度，当设置项较少时（如一级设置页或限制级设置页），Flex 子项从页面顶部开始排列而失去了原有的整体垂直居中视觉美感。
  2. **设置项选中时白色图形左侧被裁切（Task 2）**：`.settings-level-view` 设置了 `overflow: hidden`，而 `.settings-list-group` 仅有 `18px` 的左右内边距（`padding: 14px 18px 28px 18px; margin: -14px -18px 0 -18px;`）。当 `.settings-focus-pill` 执行 `scale(1.015)` 放大并叠加 `0 8px 24px` 阴影时，其左侧圆角与外扩阴影超出了 `18px` 缓冲区，被外层滚动容器和 `.settings-level-view` 直接横向裁切。
- **修复方案**：
  1. **自适应垂直居中与长列表标题冻结兼顾**：将 `.fs-settings-page` 恢复为 `height: auto; max-height: 100vh; margin: auto auto auto 0; overflow: visible;`，并在 `autoScaleUI()` 中根据当前 `overlayZoom` 动态同步 `settingsPage.style.maxHeight = Math.floor(window.innerHeight / overlayZoom) + 'px'`。当设置项较少时，`.fs-settings-page` 高度自适应内容并通过 `margin: auto auto auto 0` 将大标题与设置项整体垂直居中；当设置项较多触及 `max-height` 时，`.fs-page-header` 固定在顶部冻结，仅 `.settings-list-group` 内部滚动。
  2. **拓宽横向缓冲区消除左侧裁切**：将 `.settings-level-view` 改为 `overflow: visible`，并将 `.settings-list-group` 扩展为 `width: calc(100% + 96px); margin: -20px -48px 0 -48px; padding: 20px 48px 32px 48px;`（对应 `.settings-focus-pill` 设为 `left: 48px; width: calc(100% - 96px);`），为白色胶囊缩放与阴影预留充足的 `48px` 横向安全边界，彻底消除左侧裁切。

#### (二) 流媒体视频无法加载、黑屏暂停且无视频时长的回归故障深度剖析与修复（Task 3）
- **故障现象**：
  上一版本（Round 25）中流媒体所有视频均可正常加载播放，但在 Round 26（`v3.0.0`）中大量流媒体视频点开后直接黑屏、处于暂停状态，且右下角显示不出视频时长（`--:--`）。
- **三大底层回归根因**：
  1. **【前端根因：`executeItemAction` 误将 `currentPlayingStream` 与 `macVideo.src` 作为背景流已启动依据，跳过 `start_ffmpeg` 与 `probeStreamDuration`】**：
     - 当用户将焦点移动到某张流媒体卡片时，`setFocus()` 会**立即**将 `currentPlayingStream = targetStreamUrl` 赋值，但真正的背景流启动函数 `startPreviewStream()` 处于 `320ms` 的 `bgPreviewTimeout` 防抖延时器中（此时 `currentBgUrl` 仍为空或旧地址，尚未调用 `start_ffmpeg`）。
     - Round 26 在 `executeItemAction()` 中将复用判断改写为 `var activeBgStreamUrl = currentBgUrl || currentPlayingStream` 且條件加入了 `|| (macVideo && macVideo.src)`。当用户在聚焦卡片后按回车进入正式播放页时，`executeItemAction()` 先执行了 `clearTimeout(bgPreviewTimeout)`（取消了尚未触发的预览流请求），随后因 `currentPlayingStream === url` 且 `macVideo.src` 残留上一个视频的 Blob URL，错误判定 `isReusingActiveBgStream = true` 并直接 `return` 提前退出！导致既没有调用 `probeStreamDuration` 获取视频时长，也没有调用 `invoke('start_ffmpeg')` 启动转码流，画面直接黑屏暂停且无时长！
  2. **【后端根因 A：跳过 `probe_douyin_candidates` (`ffprobe`) 导致无内联时长且混入伪装的 10-bit ByteVC1/ByteVC2 流】**：
     - Round 26 在 JS `pickBestPlayUrl()` 中直接给 URL 加上 `h264:` 前缀并在 Rust `on_document_title_changed` 中跳过了 `probe_douyin_candidates`。一方面，部分抖音 `play_addr` 虽未标记 `is_bytevc1` 实则为 10-bit HEVC/ByteVC2 或已失效 CDN 节点；另一方面，跳过 `ffprobe` 导致当 JS 层缺失 `duration` 字段时无法补全 `@@dur=` 时长元数据。
  3. **【后端根因 B：放开 `_SSR_DATA` / `renderData` 静态快照扫描混入未签名/过期预构建流地址】**：
     - Round 26 放开了 `deepScan` 对 `_SSR_DATA` / `renderData` / `initialState` 的拦截并新增 `scanInitialDouyinHydration()`，将网页 SSR 静态快照中未携带完整实时防盗链签名或已过期的预构建 URL 推入推荐流，导致 FFmpeg 拉流时直接返回 `403 Forbidden`。
- **修复方案**：
  1. **前端严格还原 `currentBgUrl` 真实活动流判定（`src/index.html` -> `executeItemAction` / `setFocus`）**：`isReusingActiveBgStream` 严格要求 `currentBgUrl && isSameMediaStream(currentBgUrl, url)` 且 `(inFlightFfmpegReq || hlsInstance)` 真实存在，彻底移除 `currentPlayingStream` 与 `macVideo.src` 的伪复用条件，确保未启动背景流时 100% 正常执行 `probeStreamDuration` 与 `invoke('start_ffmpeg')`。
  2. **后端恢复 Round 25 经过验证的 `probe_douyin_candidates` 预检管线与 SSR 静态快照隔离（`src-tauri/src/lib.rs`）**：移除 JS 层未经验证的 `h264:` 直通前缀与 `scanInitialDouyinHydration()`，恢复 `deepScan` 对 `_SSR_DATA` / `renderData` / `initialState` 的过滤，并恢复 `on_document_title_changed` 中 `probe_douyin_candidates(&play_url)` 的 `ffprobe` 8-bit H.264 校验与真实时长探测，同时完整保留 `window.fetch` 对 JSON `Content-Type` 的精准克隆守卫。

#### (三) 光影主题轻微降亮、侧边栏自动隐藏崩溃与联动左移修复及侧边栏边框优化（Task 4, Task 5, Task 6）
1. **轻微降低“光影”主题亮度（Task 4）**：
   - 在 `renderPspWaveFrame()` 的 `bgStyle === 'light'` 分支中，将 5 组流动光斑的基础不透明度从 `0.34 / 0.28 / 0.26 / 0.22 / 0.24` 下调至 `0.22 / 0.18 / 0.17 / 0.14 / 0.15`，并将径向渐变中心 RGB 提亮幅度从 `+28` 降至 `+10`、中段衰减系数从 `0.62` 降至 `0.52`，使光影质感更温润深邃、不抢夺前景文字与海报视觉重心。
2. **左侧边栏“自动隐藏”功能崩溃修复与标题/文字/卡片联动左移（Task 5）**：
   - **故障根因**：Round 26 在 `syncSidebarCollapseState(col)` 的 `appSettings.sidebarMode === 'autohide'` 分支第 2511 行编写了 `var hideSidebarNow = (!isOnSeasonBar && curCol >= 2);`，其中 `isOnSeasonBar` 是**未声明的未定义变量**！一旦开启“自动隐藏”模式，每次调用 `setFocus(targetEl)` 执行到第 9057 行 `syncSidebarCollapseState(col)` 时立即抛出未捕获异常 `ReferenceError: isOnSeasonBar is not defined` 并中断 `setFocus()` 后续逻辑：
     - 第 2513 行 `sidebar.classList.add('autohidden')` 与 `mainContent.classList.add('sidebar-autohidden')` 永远无法执行，导致**侧边栏无法收起，标题和文字信息也未左移**；
     - 第 9174 / 9213 行 `updateHeroUI(...)` 永远无法执行，导致一旦流媒体后台回调 `updateStreamingHeroHeader()` 将 `#heroTitle` 设为 `"抖音精选"` 后，切换到任何其他板块或卡片时**所有标题均卡死显示为“抖音精选”**；
     - 第 9221 行 `track.style.transform = 'translate3d(-' + offset + 'px, 0, 0)'` 永远无法执行，导致**焦点一直向右移动时卡片轨道完全不向左滚动，焦点直接离开画面范围**。
   - **修复方案**：在 `syncSidebarCollapseState(col, row)` 中显式解析 `curRow` 并定义 `var onSeasonBar = Boolean(curRow === 0 || (currentFocusEl && currentFocusEl.classList && currentFocusEl.classList.contains('season-pill')));`，以 `(!onSeasonBar && curRow >= 1 && curCol >= 2)` 驱动 `.sidebar.autohidden` 与 `.main-content.sidebar-autohidden`（`left: 0; width: 100%; transform: translateX(0);`）。当侧边栏向左彻底隐藏时，`#mainContent` 内的 `#heroText`（大标题、副标题、剧集简介、品牌标识）、`#seasonBar` 与 `#carouselContainer`（卡片轨道及左侧渐变遮罩）同步平滑左移至屏幕左边缘。
3. **左侧边栏边框弱化与融合优化（Task 6）**：
   - 将 `.sidebar` 的外边框由 `1px solid rgba(255, 255, 255, 0.11)` 调细并调暗为 `0.5px solid rgba(255, 255, 255, 0.042)`，同时移除高对比度的顶部亮白内阴影，改为极柔和的 `inset 0 0 0 0.5px rgba(255, 255, 255, 0.025)`，彻底消除侧边栏边缘的生硬突兀感。

### 20. 菜单键双向唤出/收起侧边栏、丸形菜单保持侧边栏状态、侧边栏新增“始终隐藏”方案、macOS 27 Golden Gate 系统级 Liquid Glass 液态玻璃侧边栏设计及 v4.0 首个正式版打包（Round 28 重点复盘 Task 1–6，v4.0）

#### (一) 菜单键（`F16`）支持双向唤出与收起侧边栏及焦点妥善落位（Task 1）
- **交互诉求**：
  菜单键（`F16`）不仅用于唤出侧边栏，还可用于收起侧边栏，且收起侧边栏后需将焦点妥善放置在合适的位置。
- **实现方案（`src/index.html` -> `handleF16ContextMenuKey` / `syncSidebarCollapseState`）**：
  1. **当焦点位于卡片或丸形菜单（`fCol >= 1 || fRow === 0`）时按下菜单键（`F16`）**：清除 `sidebarForceHiddenByMenu`，唤出左侧边栏并将焦点移至当前板块对应的侧边栏导航项（`col = 0`）。
  2. **当焦点位于左侧边栏（`fCol === 0 && fRow !== 0`）时按下菜单键（`F16`）**：标记 `sidebarForceHiddenByMenu = true` 收起左侧边栏，自动解析当前所属的内容板块行（若当前位于“用户头像”、“设置”或“电源”等非列表项，则自动恢复最近浏览的媒体内容行 `lastActiveMediaRow` 并调用 `renderTrack(targetMediaRow)`），将焦点精准放置在 `getRememberedCard(targetMediaRow)`（当前板块记忆卡片或首张卡片）上。

#### (二) 焦点从卡片移动到丸形菜单时保持侧边栏状态，仅回到左侧边栏时才唤出（Task 2）
- **交互诉求**：
  当焦点从卡片移动到丸形菜单（`#seasonBar`）时无需唤出左侧边栏，只有当焦点真正回到左侧边栏（`col === 0`）时才唤出侧边栏。
- **实现方案（`src/index.html` -> `syncSidebarCollapseState`）**：
  - 在 `syncSidebarCollapseState(col, row)` 中显式区分 `onSeasonBar`（`row === 0`）与 `isOnSidebar`（`!onSeasonBar && curCol === 0`）：
    - 在“自动隐藏”（`autohide`）模式下，当 `onSeasonBar` 为真时保持 `.autohidden` 状态不变，绝不因进入丸形菜单而唤出侧边栏；
    - 在“自动折叠”（`auto`）模式下，仅当 `isOnSidebar` 为真时才展开侧边栏，焦点位于卡片或丸形菜单时均保持折叠态。

#### (三) 左侧边栏新增第四方案“始终隐藏”（`alwayshide`，Task 3）
- **交互诉求**：
  在侧边栏折叠方案中新增 **“始终隐藏”**（`alwayshide`）。开启该方案后，侧边栏始终隐藏，即使焦点位于第一个卡片（`col === 1`）也不自动唤出左侧边栏；只有用户按下菜单键（`F16`）才会唤出左侧边栏并将焦点放置在当前所在的导航项。
- **实现方案（`src/index.html` -> `syncSidebarCollapseState` / `keydown` / `initApp` / `closeSettingsModal`）**：
  1. 在【设置 -> 个性化设置 -> 左侧边栏折叠方案】中扩展四档循环切换：`自动折叠` (`auto`) / `始终折叠` (`always`) / `自动隐藏` (`autohide`) / `始终隐藏` (`alwayshide`)。
  2. 当 `appSettings.sidebarMode === 'alwayshide'` 时，只要焦点不在左侧边栏上（`!isOnSidebar`），无论焦点位于第一个卡片（`col === 1`）、后续卡片（`col >= 2`）还是丸形菜单（`row === 0`），均强制施加 `.sidebar.autohidden` 与 `.main-content.sidebar-autohidden`。
  3. 键盘导航隔离：在 `'alwayshide'` 模式下，在第一个卡片按左方向键（`ArrowLeft`）、在任意卡片按返回键（`Escape` / `Backspace`）或上下方向键，均不会将焦点移入左侧边栏，严格保证只有按下菜单键（`F16`）才会唤出左侧边栏。

#### (四) macOS 27 Golden Gate 100% 原生 Liquid Glass（液态玻璃）侧边栏设计与传统模糊自由切换（Task 4）
- **交互与视觉诉求**：
  针对 macOS 27 Golden Gate 系统适配 Apple **100% 原生 Liquid Glass（液态玻璃）** 侧边栏视觉效果，同时将原有的传统模糊侧边栏设计完整保留在代码内，并在设置中增加选项供用户自由切换使用 `Liquid Glass` 还是 `传统模糊` 设计（仅适配侧边栏，不更改其他任何组件设计）。
- **底层原生实现原理（`libcooltv_glass.dylib` + `WKMaterialHostingSupport` + `-apple-system-glass-material-clear`）**：
  1. **启用系统级 WebKit / SwiftUI 原生 Liquid Glass 托管图层**：
     - 通过 `Feel Cool TV.app/Contents/Frameworks/libcooltv_glass.dylib` 对 `WKWebView` 与 `WKPreferences` 开启 `_setUseSystemAppearance:YES`，并在 `.sidebar.liquid-glass` 上应用系统原生属性 `-apple-visual-effect: -apple-system-glass-material-clear; color-scheme: dark;`。
     - WebKit 在 UIProcess 中通过 `WKMaterialHostingSupport` 创建原生 `SwiftUI.CAHostingLayer<WebKit.MaterialHostingView>`（与 AppKit `NSGlassEffectView` / SwiftUI `.glassEffect(_:in:)` 共享完全相同的底层 `SwiftUI.SDFLayer` + `CABackdropLayer` `glassBackground` + `CASDFLayer` `CASDFOutputEffect` / `CASDFKeyFillHighlightEffect` + `CASDFElementLayer` 架构）。
  2. **严格遵循 Apple 官方规范移除自定义背景/遮罩干扰，并调优 `glassBackground` 清透光学折射参数**：
     - 严格遵循 Apple *[Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass)* 规范（*"Reduce your use of custom backgrounds in controls and navigation elements. Any custom backgrounds and appearances you use in these elements might overlay or interfere with Liquid Glass"*），在 `.sidebar.liquid-glass` 下彻底清除自定义 CSS `background`、`backdrop-filter`、`-webkit-mask-image`、`border`、`box-shadow` 及 `::before`/`::after` 伪元素（正是此前残留的自定义 SVG `feSpecularLighting` 与 CSS 背景导致 `CAHostingLayer` 绘制了未裁切的灰色矩形底框）。
     - 同时在 `libcooltv_glass.dylib` 中拦截 `+[WKMaterialHostingSupport updateHostingLayer:materialEffectType:colorScheme:cornerRadius:]` 与 `layoutSublayers`，将底层 `CABackdropLayer` 的 `glassBackground` 滤镜中会导致“高斯模糊乳白感”的 5 层渐进模糊带（`inputBlurOpacity0..3`）归零，将基础模糊半径设为清透的 `2.2`，并开启原生 SDF 边缘光学折射（`inputRefractionOpacity = 0.85`、`inputInnerRefractionAmount = -60`、`inputOuterRefractionAmount = 42`），呈现出清透高折射率的 100% 原生 Liquid Glass 质感。
  3. **设置项自由切换**：
     - 在【设置 -> 个性化设置】中新增 **“左侧边栏材质设计”**（`sidebarStyle`，持久化于 `localStorage.fc_sidebar_style`），支持在 **`Liquid Glass`**（`'liquid'`）与 **`传统模糊`**（`'blur'`）之间实时切换。

#### (五) v4.0 首个正式版单一应用程序包（`Feel Cool TV.app`）与独立运行保障（Task 6）
- **单一正式版 `.app` 规范**：
  - 清理此前因 Tauri 默认 `productName`（`cooltv-mac`）与展示名（`Feel Cool TV`）不一致而重复生成的 `cooltv-mac.app`，在项目根目录 `/Users/gaobo/Desktop/cooltv-mac/` 下**仅保留唯一正式版应用程序包 `Feel Cool TV.app`**。
  - 在 `Feel Cool TV.app/Contents/Resources/src/` 中完整内置前端静态资源，并通过 `Contents/MacOS/cooltv-mac` 启动器内置绑定主进程生命周期的 `127.0.0.1:1430` 静态资源服务与 `libcooltv_glass.dylib` 原生液态玻璃支持库，双击 `Feel Cool TV.app` 即可直接独立运行。

### 21. 顶部胶囊侧边栏设计、Liquid Glass 浅色/深色双外观、始终隐藏独立分层覆盖、流媒体主界面作者副标题、各分区独立 20 条历史额度及 v4.1.0 版本同步（Round 29 重点复盘 Task 1–9，v4.1.0）

1. **“正在观看”选中的丸形菜单始终对齐当前播放内容，杜绝从很酷 TV+ 返回主页时误触发流媒体预览打断播放（Task 1 & Task 7）**：
   - 新增 `syncActiveNowWatchingTabToCurrentPlayback()`，每当开始播放视频或按主页键（`F18`）返回“正在观看/现在观看”时，强制将 `activeNowWatchingTab` 对齐至当前正在播放内容的所属分区（`cooltv` / `iptv` / `stream` / `restricted`），彻底消除此前因停留在“流媒体”丸形菜单而误聚焦抖音卡片打断原播放视频的问题。
2. **Liquid Glass 浅色（`light`）与深色（`dark`）双原生外观模式及左侧边栏紧凑边距优化（Task 2 & Task 3）**：
   - 在 `libcooltv_glass.dylib` 与 `.sidebar.liquid-glass` 中同时支持 `color-scheme: dark` 与 `color-scheme: light`，并在【设置 -> 个性化设置】中提供 `Liquid Glass 外观`（`深色` / `浅色`）切换；轻微收窄左侧边栏距屏幕边缘间距（`14px`）、侧边栏内按钮间距与按钮内图文边距以提升屏幕空间利用率。
3. **左侧边栏“始终隐藏”模式彻底分层覆盖与方向键/返回键收起（Task 4）**：
   - 在 `'alwayshide'` 模式下，`#mainContent` 始终保持 `.expanded.sidebar-autohidden` 全宽不位移，召唤出的左侧边栏（`z-index: 30`）作为独立浮层覆盖于页面上方而不推动下层元素；召唤侧边栏仅限菜单键（`F16`），隐藏已召唤的侧边栏支持左右方向键、返回键或菜单键。
4. **暂停态与剧集展开态下主页键（`F18`）及返回键自如往返正式播放页面，且修复很酷 TV+ 误显点赞收藏图标（Task 5 & Task 6）**：
   - 新增 `canReturnToFullscreenByF18` 状态保持，确保即使视频处于暂停状态或位于剧集展开子视图，按 `F18` 仍可无缝在正式播放页与主页面间往返切换；严格收敛 `isCurrentStreamDouyin()` 仅匹配流媒体源，消除很酷 TV+ 正式播放页误现点赞收藏图标与流媒体副标题显示“很酷 TV+”的回归问题（流媒体主页面副标题改为显示视频作者 `getCurrentStreamingHeroAuthorSub()`）。
5. **Apple tvOS 风格顶部侧边栏（`.sidebar.top-bar`）与各分区独立 20 条观看历史记录额度（Task 8 & Task 9）**：
   - 新增 `侧边栏位置`（`左侧` / `顶部`）设置项，顶部侧边栏采用居中长连贯药丸胶囊设计；通过 `enforcePerSectionHistoryQuota(list, 20)` 为很酷 TV+、有线电视、流媒体各自分配独立的 20 条历史记录配额，互不挤占覆盖。
6. **v4.1.0 版本号全链路同步与单一 `.app` 启动缓存修复**：
   - 同步更新 `src/index.html`（`#settingsVersionText` 静态节点与 `vFooterEl.textContent` 动态赋值）、`Feel Cool TV.app/Contents/Resources/src/index.html`、`Feel Cool TV.app/Contents/Info.plist`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 及 `package.json` 版本号至 `4.1.0`，清理 `src-tauri/target/release/bundle/macos/` 下残留的旧版 `4.0` 应用包，并加固启动器静态文件服务 `Cache-Control: no-store` 与最新文件校验。

### 22. 顶部侧边栏正式播放页自动隐藏与“始终隐藏”方案、正式播放页播放按钮微放大与 Liquid Glass 化、设置页保留动量平滑滚动引擎、Apple TV (tvOS) 深色设置页毛玻璃遮罩设计升级及气象信息避让适配（Round 30 重点复盘 Task 1–7，v4.2.0）

1. **顶部侧边栏在进入正式播放页面时自动隐藏，且正式播放页面严禁唤出顶部侧边栏（Task 1 & Task 3）**：
   - **根因剖析**：Round 29 在 `.sidebar.top-bar` 上编写了 `opacity: 1 !important; pointer-events: auto !important; transform: translate3d(-50%, 0, 0) !important;`，导致 `executeItemAction()` 进入正式播放页面（`isFullScreen = true`）时设置的 `sidebar.style.opacity = '0'` 被 `!important` 覆盖失效。
   - **解决方案**：移除 `.sidebar.top-bar` 基础态上的 `opacity` / `pointer-events` / `transform` `!important` 锁死，新增 `.sidebar.fullscreen-hidden` 与 `.sidebar.top-bar.fullscreen-hidden`（`transform: translate3d(-50%, -88px, 0) !important; opacity: 0 !important; pointer-events: none !important; visibility: hidden !important;`），在进入正式播放页面时施加 `fullscreen-hidden` 并在退出时恢复；同时 `handleF16ContextMenuKey()` 在 `isFullScreen === true` 时仅控制全屏选集/流媒体菜单与播放进度条，严禁在正式播放页面唤出顶部或左侧边栏。
2. **正式播放页面播放状态按钮（`#fsPlayStateBtn`）轻微放大并赋予原生 Liquid Glass 设计（Task 2）**：
   - 将 `.fs-play-state-btn` 尺寸从 `34px × 34px`（内部 SVG `18px × 18px`）轻微放大至 `42px × 42px`（内部 SVG `22px × 22px`），注入 `-apple-visual-effect: -apple-system-glass-material-clear; color-scheme: dark;`（并随 `appSettings.liquidGlassTheme` 同步切换 `.liquid-light` / `.liquid-dark`），结合 `libcooltv_glass.dylib` 对圆形图层的 `effectiveRadius = h * 0.5` 自动识别与无自定义背景遮挡规范，呈现清透高折射率的圆形原生液态玻璃质感。
3. **为顶部侧边栏开放“始终隐藏”方案（Task 3）**：
   - 当侧边栏位置设为 `顶部` 时，`sidebarMode` 支持在 **`始终显示`** 与 **`始终隐藏`**（`'alwayshide'`）之间切换。开启“始终隐藏”后，顶部侧边栏默认向上平移淡出隐藏（`.sidebar.top-bar.autohidden`），启动 APP 或按 `F18` 返回主页时自动聚焦首张卡片以保持侧边栏隐藏，在卡片或丸形菜单按 `ArrowUp` / `Escape` / `Backspace` 均不唤出顶部侧边栏；唯有在非正式播放页面按下菜单键（`F16`）才唤出顶部侧边栏并将焦点放置在当前所在的导航项上，在顶部侧边栏按下键（`ArrowDown`）、返回键（`Escape` / `Backspace`）或菜单键（`F16`）则重新收起隐藏。
4. **设置页面保留动量的临界阻尼平滑滚动引擎与滚动边缘渐隐遮罩（Task 4）**：
   - 移除 `.settings-list-group` 上连续按键会重置初速度造成卡顿的原生 `scroll-behavior: smooth`，新增基于 `requestAnimationFrame` 时间步长指数阻尼（`1 - Math.pow(0.0008, dt / 1000)`）的平滑滚动引擎 `animateSettingsListScrollTo()`，连续按上下键时持续累加更新目标位置并完整保留滚动动量；同时引入 `topPad` / `botPad` 预判舒适区与 `.has-scroll-top` / `.has-scroll-bottom` 上下边缘渐变透明遮罩（`updateSettingsListScrollEdgeMasks()`），并支持鼠标滚轮/触控板即时打断接管。
5. **参照 Apple TV (tvOS) 深色设置页面（`systemThickMaterialDark`）升级设置页、电源选项页与密码页背景遮罩设计（Task 5）**：
   - 依据 Apple Human Interface Guidelines 的 tvOS 深色材质规范，将 `.fs-overlay-mask`（统一承载 `#settingsModal`、`#powerModal`、`#passwordModal` 及 `#tvShuttingDownModal`）升级为 `backdrop-filter: blur(78px) saturate(185%) brightness(0.82) contrast(1.04)`，配合双层环境色光晕（`--overlay-ambient-glow`、`--overlay-secondary-glow`）与三段式半透明石墨深灰渐变（`--overlay-bg-inner: 0.62` -> `--overlay-bg-mid: 0.72` -> `--overlay-bg-outer: 0.80`）及顶部微高光内描边（`inset 0 1px 0 rgba(255, 255, 255, 0.048)`），既彻底融化底层画面硬边又透出温润饱满的色彩层次。
6. **顶部气象信息（`#jpWeatherTelop`）动态避让顶部侧边栏适配（Task 6）**：
   - 新增 `syncWeatherTelopPosition()`，当顶部侧边栏处于可见状态（`top-bar` 且非 `autohidden`、非 `fullscreen-hidden`、非 `isFullScreen`）时，根据当前 UI 缩放系数动态计算 `--telop-top-with-bar = Math.max(88, Math.round(62 * scale + 20)) + 'px'` 并施加 `.top-bar-avoid`，使气象信息平滑下移至顶部胶囊侧边栏正下方；当顶部侧边栏隐藏或进入正式播放页面时平滑回升至 `48px`。

### 23. 丸形菜单与选集列表 Liquid Glass 化、侧边栏聚焦时隐藏副标题与小标题、“流媒体”更名为“短视频”及作者/媒体源标题重构、顶部侧边栏偶发消失修复与 1918×802 本地媒体解码修复（Round 31 重点复盘 Task 1–7，v4.3.0）

1. **丸形菜单（`.season-pill`）与选集列表（`.fs-ep-item-btn`、`.fs-ep-season-arrow`）适配原生 Liquid Glass 设计（Task 1）**：
   - 为 `.season-pill`、`.fs-ep-item-btn` 与 `.fs-ep-season-arrow` 的非聚焦态注入 `-apple-visual-effect: -apple-system-glass-material-clear` 原生液态玻璃属性并清除自定义毛玻璃与实线边框，同步支持 `body.liquid-glass-light` / `body.liquid-glass-dark` 深浅色外观切换；当前选中未聚焦态（`.current:not(.focused)`）采用轻薄高光内描边标识，聚焦态（`.focused`）切换为纯白高亮胶囊。
   - 升级 `libcooltv_glass.dylib`（`swizzled_updateHostingLayer`），对所有 `h > 10.0 && h <= 128.0 && w >= h - 4.0` 的胶囊与圆形图层统一赋予精确的 `effectiveRadius = h * 0.5` 半高胶囊圆角。
2. **焦点位于侧边栏导航项时隐藏当前板块副标题（`#heroSub`）和小标题（`#heroBrand`），移至首个卡片时恢复显示（Task 2）**：
   - 在 `updateHeroUI()` 中判定当前焦点位置：当焦点位于侧边栏导航项（`col === 0` 或丸形菜单 `row === 0`，且非“正在观看/现在观看”板块 `row !== 7`）时，自动隐藏副标题（`#heroSub`）、简介（`#heroDesc`）与小标题（`#heroBrand`），仅展示大标题（`#heroTitle`）；当焦点移动到板块内第一个卡片（`col >= 1`）时立即恢复副标题与小标题显示，“正在观看/现在观看”板块保持原有展示逻辑不变。
3. **“流媒体”全面更名为“短视频”，并重构短视频大标题与副标题信息层级（Task 3 & Task 4）**：
   - 将侧边栏导航项（`#navDouyinItem`）、正在观看丸形菜单分类（`NOW_WATCHING_BASE_TABS`）、设置页入口（`短视频账号连接`）及正式播放页菜单（`#fsStreamMenuTitle` `短视频选项`）中的“流媒体”统一更名为“短视频”。
   - 在**短视频板块（`row === 3`）**中，当焦点位于卡片上时，卡片上方大标题（`#heroTitle`）改为显示作者（如 `@李子柒`），副标题（`#heroSub`）改为显示纯粹媒体源（`抖音` 或 `bilibili`，不带“精选”或“热门”等分类后缀）；卡片下方标题与正式播放页面标题保持视频标题不变。
   - 在**正在观看/现在观看板块（`row === 7`）**中，短视频内容的副标题统一显示为 `短视频社区 · 媒体源`（如 `短视频社区 · 抖音`、`短视频社区 · bilibili`），同时在卡片 `data-org` 中保留作者信息以确保从正在观看进入正式播放页面时 HUD 副标题仍显示作者。
4. **排查并根治顶部侧边栏偶见“选中状态消失”或“整个侧边栏消失”异常（Task 5）**：
   - **根因 1（选中状态消失）**：`setFocus()` 在处理丸形菜单（`row === 0`）分支时提前 `return`，未执行底部的 `.active-row` 同步逻辑，导致焦点从顶部侧边栏下移至丸形菜单时 `#sidebarFocusPill` 隐藏且 `.active-row` 未挂载，顶部侧边栏选中状态完全消失。**修复**：将 `.active-row` 高亮同步提升至 `setFocus()` 顶部（在 `row === 0` 分支之前执行），并为 `.sidebar.top-bar .nav-item.active-row:not(.focused)` 增加高对比半透明胶囊内描边高亮；同时在 `syncSidebarFocusPill()` 中增加首帧尺寸未就绪时的 `requestAnimationFrame` 自动补测以及 `updateNowPlayingNavVisibility()` 文案变更后的重同步。
   - **根因 2（整个顶部侧边栏消失）**：① 顶部侧边栏处于“始终显示”模式时，按 `F16` 或从“始终隐藏”切回“始终显示”会残留 `sidebarForceHiddenByMenu = true`，触发 `.autohidden` 导致整个顶部侧边栏上移消失；② `.sidebar.fullscreen-hidden` 使用了 `visibility: hidden !important`，导致 WebKit `WKMaterialHostingSupport` 在进入正式播放页面时销毁原生 Liquid Glass `CAHostingLayer`，退出全屏时偶发未能重建图层。**修复**：在顶部侧边栏“始终显示”模式下强制重置 `sidebarForceHiddenByMenu = false` 并移除 `.autohidden`；将 `.sidebar.fullscreen-hidden` 改为纯屏幕外位移与 `opacity: 0`，移除 `visibility: hidden` 以保持原生 `CAHostingLayer` 常驻。
5. **根治 `1918 × 802`（23.98fps H.264）等本地媒体播放画面卡住（暂停继续后恢复）与切块异常（Task 6）**：
   - **根因**：`1918 × 802` 经旧版 `trunc(iw/4)*4` 缩放后为 `1916 × 800`（宽度 `1916` 非 16 宏块整数倍，触发 SPS `frame_cropping_flag=1`，且非 16:9 画布在 WebKit `TiledCoreAnimationDrawingArea` 负层级下引发瓦片失效错位），同时旧版 `is_local_file` 分支使用 `h264_videotoolbox -realtime 1` 多 Slice 编码、未剥离源流私有 SEI，且 `start_ffmpeg` 在生成首个切片（仅 1 秒缓冲）时即返回前端起播，导致首片交界处缓冲区欠载使 `AVSampleBufferDisplayLayer` 挂起冻结。
   - **修复**：将 `is_local_file` 分支统一升级为 `scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30,format=yuv420p`（80×45 完整 16×16 宏块，`frame_cropping_flag=0`），采用 `libx264 ultrafast + zerolatency + sliced-threads=0:ref=1:bframes=0:force-cfr=1` 单 Slice 整帧编码并剥离私有 SEI（`filter_units=remove_types=6`），并在 `start_ffmpeg` 中等待至少 3 个完整切片（`_0002.ts`）就绪后再挂载起播。

### 24. 短视频与正在观看短视频大标题“作者名 + 的视频”化、短视频副标题“来自 + 媒体源及分区”化、设置/电源聚焦保持原页面内容、macOS 顶部菜单栏汉化、很酷 TV+ 大标题简化及定制 App Icon 部署（Round 32 重点复盘 Task 1–6，v4.4.0）

1. **短视频板块（`row === 3`）大标题显示“作者名 + 的视频”，副标题显示“来自 + 媒体源及分区”（Task 1）**：
   - 新增 `formatShortVideoAuthorHeroTitle(rawAuthor)` 与 `getShortVideoCategorySourceSub(elOrItem)`：当焦点位于短视频板块卡片上时，卡片上方大标题（`#heroTitle`）显示为 `作者名 + 的视频`（例如 `@李子柒 的视频`），副标题（`#heroSub`）由纯粹媒体源升级为 `来自 + 媒体源及分区`（例如 `来自 抖音精选`、`来自 bilibili热门`，自动联动 `.douyin-font` 品牌字体与 `.inline-bili-logo` 内联图标）；正式播放页面标题与副标题保持不变。
2. **正在播放/现在观看板块（`row === 7`）下的短视频内容大标题显示“作者名 + 的视频”，副标题保持不变（Task 2）**：
   - 修复此前“正在观看/现在观看”板块下短视频内容大标题误显示为单条视频标题的问题：在 `setFocus()`（含侧边栏 `col === 0 && row === 7` 与卡片 `row === 7 && effRow === 3`）及 `updateNowWatchingHeroHeader()` 中，针对 `sourceRow === 3` 的短视频卡片统一提取作者名（`org`）并调用 `formatShortVideoAuthorHeroTitle()` 显示为 `@作者名 的视频`，副标题保持 `短视频社区 · 媒体源`（如 `短视频社区 · 抖音`）不做任何改变。
3. **当焦点位于侧边栏“设置”（`row === 5`）或“电源”（`row === 6`）时，标题和卡片停留在之前的内容上不做改变（Task 3）**：
   - 为侧边栏“设置”与“电源”导航项添加 `data-no-bg-change="true"` 属性，并移除 `setFocus()` 中针对 `row === 5 || row === 6 || row === -1` 调用 `renderTrack(-1)` 清空轨道的旧逻辑，使焦点移至“设置”或“电源”时主页面大标题、副标题、小标题、丸形菜单及卡片轨道均完整保持此前所在板块的内容。
4. **应用程序对应的 macOS 顶部系统菜单栏全面汉化（Task 4）**：
   - 在 `libcooltv_glass.dylib`（`translateMenuTitleToChinese` / `localizeMenuRecursive` / `swizzled_setMainMenu`）与 `src-tauri/src/lib.rs`、`Feel Cool TV.app/Contents/Info.plist`（`CFBundleDevelopmentRegion = zh_CN`、`CFBundleLocalizations = [zh_CN, zh-Hans]`）及 `zh-Hans.lproj/InfoPlist.strings` 中实现双层汉化保障：将应用主菜单（`关于 Feel Cool TV`、`服务`、`隐藏 Feel Cool TV`、`隐藏其他`、`全部显示`、`退出 Feel Cool TV`）及 `文件`、`编辑`、`显示`、`窗口`、`帮助` 各级子菜单全面汉化为标准 macOS 简体中文文案。
5. **很酷 TV+ 的大标题不再显示“剧集”或“电影”字样（Task 5）**：
   - 将 `#navCoolTvPlusItem` 的 `data-title` 与 `getCurrentCoolTvHeroTitle()` 返回值统一简化为 `'很酷 TV+'`，使聚焦“很酷 TV+”侧边栏项或切换“剧集 / 电影”二级丸形菜单时，大标题统一渲染为内联“很酷 TV+”品牌 Logo，不再附加“剧集”或“电影”后缀。
6. **指定并部署定制 App Icon（Task 6）**：
   - 基于 Icon Composer 导出的 `Icon-macOS-Default-1024@1x.png` 设计稿生成全套多分辨率图标（`icon.icns`、`icon.png`、`32x32.png`、`128x128.png`、`128x128@2x.png`、`icon.ico`），同步写入 `src-tauri/icons/` 与 `Feel Cool TV.app/Contents/Resources/icon.icns`。

### 25. 深邃空间交互音效引擎、短视频跨轮刷新去重与末尾无感续载、二级丸形菜单首项搜索框、多页“只看作者”列表、过期视频流自动续期、B 站账户兴趣推荐流对齐、暂停态跑马灯 10 轮自动停止及 Dock 图标锁定（Round 33 重点复盘 Task 1–9，v4.5.0）

1. **Amazon Fire TV / Apple TV 风格深邃恢宏空间交互音效引擎与设置开关（Task 1）**：
   - 构建基于 Web Audio API 双振荡器（Sub-bass 正弦波 + 泛音三角波）配合低通滤波包络与立体声双延时混响网络的合成器 `CoolTvSoundEngine`（`playUiSound(type, dirHint)`），为方向键移动（`move`）、边界触底阻尼（`boundary`）、确认进入（`select`）、返回/退格（`back`）、菜单唤出（`menu`）、长按触发（`longpress`）、设置项切换（`toggle`）及点赞/收藏（`action`）赋予各具辨识度且深邃温润的低频空间音效；
   - 在【设置 -> 个性化设置】中新增 **“交互音效”**（`appSettings.uiSound`，持久化于 `localStorage.fc_ui_sound`），支持随时开启或关闭。
2. **修复抖音与 bilibili 每次刷新出现上一轮重复视频及卡片序列末尾无法向后继续加载更多卡片问题（Task 2）**：
   - **根因剖析**：① 已编译的 `cooltv-mac-engine` 在 Rust `on_document_title_changed` 中硬编码了 `if existing_list.len() >= 18 { return; }`，导致后端 `douyin_feed` 队列达到 18 条后静默丢弃所有后续抓取的视频，焦点移至末尾时无法追加新卡片；② `get_douyin_feed(refresh: true)` 调用了 `window.__FC_SEEN__.clear()`，清空已见 ID 集合后再刷新，导致接口返回的缓存批次与上一轮重复。
   - **解决方案**：在 `libcooltv_glass.dylib` 中注册原生 `WKScriptMessageHandler`（`window.webkit.messageHandlers.cooltvBridge`）跨 WebView 高速桥接通道，完全绕开 `cooltv-mac-engine` 的 18 条上限；同时在 `douyin_scraper` 与 `bilibili_scraper` 中维护跨刷新的持久化滚动去重集合（`__fc_dy_global_seen_v2__` 与 `__fc_bili_global_seen_v2__`，容量 800 条）并拦截 `window.__FC_SEEN__.clear`，确保每次刷新全部为全新视频；当焦点移至卡片序列末尾（或倒数第 3 张卡片内）按右键时，立即通过 `cooltvBridge` 触发追加加载并原地无感续接新卡片，无需重头覆盖。
3. **短视频二级丸形菜单首项搜索框（Task 3）**：
   - 在短视频展开的二级丸形菜单首位（`data-col="1"`）内置 Liquid Glass 搜索框（`#streamSearchPill` / `#streamSearchInput`）；
   - 当焦点位于搜索框时自动进入输入状态（`.editing`），实时过滤并屏蔽所有中英文标点符号；搜索框非空时按返回键（`Escape` / `Backspace`）执行退格删除末尾字符（为空时按返回键折叠回一级丸形菜单），按确认键（`Enter`）即时触发当前平台（抖音或 bilibili）关键词视频搜索。
4. **修复“只看作者”功能不可用或仅加载 1~2 个视频问题，实现完整作品列表多页抓取与流畅预加载（Task 4）**：
   - **根因剖析**：旧版 `get_stream_author_videos` 仅截取前 12 条并逐条串行运行 `ffprobe`，而主线程仅等待 `1.17s` 即超时返回，导致只返回 0~2 个视频。
   - **解决方案**：在 `libcooltv_glass.dylib` 中实现 `__FC_BRIDGE_AUTHOR_DY__`（结合 `aweme/detail` 反查 `sec_uid`、多页 `/aweme/v1/web/aweme/post/` 游标翻页及页面状态扫描兜底）与 `__FC_BRIDGE_AUTHOR_BILI__`（结合 `view` 反查 `mid`、多页 `wbi/arc/search` 与公开空间兜底），单批次拉取 3 页并过滤 H.264 直链，同时控制内存列表上限（120 条）并在浏览至列表尾部前自动静默预加载下一页，兼顾作品完整性与流畅度。
5. **修复短视频推荐流长时间未观看未刷新后已加载卡片无法正常播放问题（Task 5）**：
   - 新增 `isDouyinStreamUrlExpired(url)` 自动解析 CDN URL 中的 `x-expires` / `expires` 时间戳；当检测到已加载卡片 URL 临近或已经过期，或在 `start_ffmpeg` 起播失败时，自动调用 `refreshExpiredDouyinStreamUrl(item)`（通过 `cooltvBridge` 请求 `/aweme/v1/web/aweme/detail/` 或按标题检索）实时换取最新有效 CDN 地址，并原地更新 `ROW_DATA[3]`、DOM 卡片 `data-url` 与观看历史记录后无缝续播。
6. **修复 bilibili 推荐流与用户个人账户首页兴趣不符问题（Task 6）**：
   - 针对 bilibili 核心登录凭据 `SESSDATA` 为 `HttpOnly`（`document.cookie` 无法读取）的问题，在 `libcooltv_glass.dylib` 中直接通过 `[[WKWebsiteDataStore defaultDataStore] httpCookieStore]` 提取全量 `.bilibili.com` Cookie（含 `SESSDATA`、`bili_jct`、`DedeUserID`）同步至 `/tmp/cooltv_bili_cookie.txt` 与抓取器，并通过 `__FC_BRIDGE_FETCH_BILI__` 在已登录的 `bilibili_scraper` WebView 同源上下文内优先拉取 `x/web-interface/wbi/index/top/feed/rcmd` 个性化首页推荐流与页面 SSR 推荐流，使推荐内容与电脑浏览器访问 `bilibili.com` 首页的个人兴趣完全一致。
7. **正式播放页面暂停态视频标题跑马灯限制为 10 轮自动停止（Task 7）**：
   - 在 `startMarqueePingPong(el)` 与 `toggleFullscreenPlayPause()` 中新增暂停态轮次计数器（`el.__fcPausedLoops`）：当视频处于暂停状态（`isPausedIptv` 或 `macVideo.paused`）时，仅对正式播放页面底部标题（`#iptvHudTitle`）统计跑马灯滚动轮数，满 **10 轮**后自动在起始对齐位平滑停止动画；恢复播放或切换视频时立即重置计数并恢复滚动，其余位置（如主页面大标题、卡片标题）的跑马灯不受影响。
8. **修复 App Icon 打开 1 秒后被 Tauri 原始图标覆盖问题（Task 8）**：
   - **根因剖析**：`cooltv-mac-engine` 在 Tauri 应用启动约 1 秒后完成窗口初始化时，会调用 `[NSApp setApplicationIconImage:]` 将内嵌在二进制中的默认 Tauri 图标写入 Dock。
   - **解决方案**：在 `libcooltv_glass.dylib` 中通过 Objective-C Runtime Method Swizzling 拦截 `-[NSApplication setApplicationIconImage:]`（`swizzled_setApplicationIconImage`），并在启动与前 5 秒周期守护中调用 `enforceCustomAppIconNow()` 强制加载锁定 `Feel Cool TV.app/Contents/Resources/icon.icns`，彻底杜绝图标回退。

### 26. 抖音多路搜索修复、搜索框等高与放大镜左移优化、搜索框返回键纯退格防失焦、bilibili 一级丸形菜单默认显示子分类名及交互音效响度增强与音量调节（Round 34 重点复盘 Task 1–6，v4.6.0）

1. **修复抖音关键词搜索无法搜索出内容问题（Task 1）**：
   - **根因剖析**：
     1) 抖音网页版搜索接口（`/aweme/v1/web/search/item/` 与 `/aweme/v1/web/general/search/single/`）校验 `Referer` 必须为 `/search/<keyword>` 路由，且其 SSR 阶段生成的 `self.__pace_f.push([1, "..."])` 载荷为多行换行分隔的 React Server Components (RSC) 流（每行形如 `<hex_id>:<json>`），此前直接对整段字符串截取首个冒号后调用 `JSON.parse` 会在首行换行处抛出 `SyntaxError`；
     2) 此外，当 `activeDouyinCategory === 'dy_search'` 时，`replenishOneStreamingCardOnPlay()` 与后台轮询调用了 `invoke('get_douyin_feed', { category: 'dy_search' })`，导致后端将非 `bili_` 前缀的 `'dy_search'` 误映射回 `https://www.douyin.com/discover` 并强制重载了 `douyin_scraper`。
   - **解决方案**：
     1) 在 `libcooltv_glass.dylib` 中升级 `__FC_BRIDGE_SEARCH_DY__` 与 `/search/` 页面实时拦截器：先通过 `history.replaceState` 切换同源 `/search/<kw>?type=video` 路径以满足 `Referer` 与 `bdms` 签名校验并发起双接口检索，同时实现按行切分解析 RSC 流的函数 `parsePaceChunkString()`，并在需要时导航至真实 `/search/<kw>?type=video` 页面配合 `localStorage` 跨页状态（`__fc_dy_active_search__`）与 XHR/fetch/`__pace_f` 钩子实时回传搜索结果；在 `formatAwemeNode(aw, true)` 中放宽搜索结果对 `aw.images` 及 HEVC 候选源的过滤；
     2) 在 `executeStreamKeywordSearch()` 与 `replenishOneStreamingCardOnPlay()` 中隔离 `dy_search` / `bili_search` 与常规推荐流轮询，避免 `get_douyin_feed` 重载打断搜索。
2. **修复搜索框高度被压扁问题并将放大镜图标左移（Task 2）**：
   - 将 `.season-pill.stream-search-pill` 的内边距由 `padding: 0 15px` 修正为与相邻丸形菜单一致的 `padding: 6px 14px 6px 10px; min-height: 31px; box-sizing: border-box;`，并将内部 `.stream-search-input` 设为 `height: 19px; line-height: 19px; -webkit-appearance: none;`，使搜索框高度与所有相邻丸形按钮完全一致；同时缩小左侧内边距并为 `.stream-search-icon` 设置 `margin-left: -2px`，使放大镜符号更靠左。
3. **修复搜索框退格到最左侧时失焦问题，确保返回键仅用于退格、退出需用方向键（Task 3）**：
   - **根因剖析**：① 全局 `window` 捕获阶段的 `Escape` 监听器无差别对所有 `INPUT` 执行了 `document.activeElement.blur()`；② 搜索框 `keydown` 处理在字符删空（`curSearchVal.length === 0`）后再次收到返回键时执行了折叠二级丸形菜单并移走焦点的分支。
   - **解决方案**：在 `window` 捕获监听器中排除 `#streamSearchInput`，移除搜索框为空时按返回键折叠二级菜单的逻辑，并为 `#streamSearchInput` 挂载 `blur` 自动重聚焦守卫（当 `currentFocusEl === searchPillEl` 时保持输入焦点），确保搜索框激活时按返回键（`Escape` / `Backspace`）只用于退格，退出搜索框必须使用方向键将焦点移至他处。
4. **短视频板块下 bilibili 一级丸形按钮默认显示当前子菜单项名（Task 4）**：
   - 将 `biliSubExplicitlyChosen` 默认设为 `true`，并更新 `renderDouyinSeasonBar()`、`getCurrentStreamingHeroTitle()` 与 `getCurrentBiliHeroTitle()`，使短视频一级丸形菜单中的 bilibili 按钮默认显示对应的子分类名称（默认状态为 `bilibili 推荐`，切换子分类后显示对应子分类名，与 `抖音精选` 保持一致）。
5. **增强交互音效响度与视频播放穿透力，并在设置中支持交互音效音量调节（Task 5）**：
   - 在 `ensureUiAudioContext()` 与 `triggerResonantVoice()` 中引入 `1420Hz` 瞬态清晰度共振峰滤波器、`DynamicsCompressorNode` 动态压限器与高八度短促木质/琉璃瞬态泛音（`transientOsc`），大幅提升基础合成增益，并在背景或全屏视频有声播放时自动施加 `1.42x` 动态增益补偿与 `155ms` 视频微幅闪避（Audio Ducking）；
   - 在【设置 -> 个性化设置】中新增 **“交互音效音量”**（`uiSoundVolume`，支持 `20%` / `40%` / `60%` / `80%` / `100%` 五档循环调节，默认 `100%`）。

### 27. 抖音精选首屏加载与抖音多路搜索修复、全链路 CPU/GPU 功耗与风扇噪音治理及平滑流动感多方案交互音效（Round 35 重点复盘 Task 1–5，v4.7.0）

1. **根治“抖音精选”（`discover`）无法加载出内容及“抖音搜索”（`dy_search`）无结果问题（Task 1 & Task 2）**：
   - **根因剖析**：
     1) **冷启动跨会话已见集合阻塞首屏**：此前 `installDouyinPatches()` 在页面初始化 `t = 0ms` 时将 `localStorage` 中的跨会话历史 ID 集合（`__fc_dy_global_seen_v2__`）全部注入 `window.__FC_SEEN__`，导致 `ensure_douyin_scraper` 在 `2400ms` 时的 `window.__FC_SEEN__.size < 6` 首屏补拉条件恒为 `false`，且冷启动 `/discover` 首屏视频因命中历史集合被全部过滤；同时当从 `/search/` 切回 `discover` 时，后端 `state.douyin_category` 仍记录为 `"discover"`（未感知搜索页路由切换），未重新导航回 `/discover`；
     2) **桥接注入脚本语法校验加固**：上一轮压缩拼接 `libcooltv_glass.dylib` 内嵌 UTF-16LE 脚本时在末尾重复拼接了闭合块并混入一处八进制字面量，触发 WebKit JavaScriptCore 语法解析异常导致 `cooltvBridge` 在抓取器窗口内未挂载。
   - **解决方案**：
     1) 移除冷启动向 `window.__FC_SEEN__` 预填跨会话 `localStorage` 集合的操作，改为当前会话内存去重（且当单批次过滤后为空时自动放行兜底，确保 `discover` 永不为空），并在 `window.__FC_ACTIVE_CAT__` 上挂载 `Object.defineProperty` 响应式 setter：一旦从 `/search/` 切回常规分类自动导航回对应分类 URL，且在 `/discover` 首屏自动触发 `900ms` / `2400ms` 双阶段主动拉取；
     2) 在 `__FC_BRIDGE_SEARCH_DY__` 与 `installDouyinPatches()` 中挂载滞后于 `ensure_douyin_scraper` 的顶层 `JSON.parse` 深度搜索扫描器（深度上限提升至 `18`，兼容 `/aweme/v1/web/general/search/stream/` 流式分块 JSON、`/aweme/v1/web/search/item/` 与多行 RSC `__pace_f` 数据），并通过 macOS 系统内置 WebKit `JavaScriptCore` (`jsc`) 对动态库提取脚本执行 `new Function` 零误差语法校验。
2. **Mac 风扇噪音、CPU/GPU 占用与发热综合优化（不牺牲任何视觉效果，Task 3）**：
   - **消除冗余 `ffprobe` 进程风暴**：在 `window.__FC_QUEUE__.push` 通过 `cooltvBridge` 直传前端后限制队列积压（`< 3`），阻止 `douyin_scraper` 每 `35ms` 通过 `document.title` 触发数十个后台 `ffprobe` 子进程并发拉起；
   - **隐藏抓取器 WebView 智能降频**：在 `douyin_scraper` 首屏水合完成（`4.2s`）后，将后台隐藏态的 `requestAnimationFrame` 定时器从 `16ms`（62.5 FPS）降频至 `380ms`（约 2.6 FPS），将补丁巡检从 `120ms` 降至 `800ms`、B 站 Cookie 同步从 `4s` 降至 `8s`；
   - **按需轮询自动休眠（`douyinPollTimer`）**：当当前短视频分类已加载完可见配额且不再新增卡片时，自动停止 `750ms` 轮询定时器，仅在切换分类、手动刷新或移至列表末尾加载更多时按需唤醒；
   - **背景 Canvas 智能帧率与视频遮挡休眠（`renderPspWaveFrame`）**：当处于正式播放页面或背景视频已显影覆盖画面时，暂停每帧 Canvas 重绘并转为 `320ms` 低功耗检测；当动态丝带/光影背景可见时，在 120Hz ProMotion 屏幕上平滑锁定为 `36 FPS`（`27ms` 步进），在 100% 保持视觉丝滑度与画质的前提下大幅降低 GPU 光栅化负载。
3. **新增“深海流光”与“星云潮汐”平滑流动感交互音效方案，支持在设置中自由切换（Task 4）**：
   - 新增无点击瞬态的流动合成声部 `triggerFlowingSwellVoice()`：采用双微失谐纯正弦波（`±1.5Hz ~ ±2.6Hz` 空间合唱拍频）叠加低八度暖流正弦波与五度泛音，使用 `52ms ~ 110ms` S 型缓起渐入包络（彻底消除硬起音点击感）配合平滑指数滑音（Glissando）与呼吸式低通滤波扫频；
   - 在【设置 -> 个性化设置 -> 交互音效方案】中提供三种可选方案：**`深海流光`**（默认，深邃温润的暗流涌动感）、**`星云潮汐`**（空灵舒缓的五度泛音滑音与潮汐余韵）、**`深邃脉冲`**（保留原瞬态脉冲方案）。

---

## 五、组件、页面与快捷键标准称呼词典（任务 14 长期维护规范）

为确保后续每一次迭代指令与代码实现 100% 精准对齐，本章详细记录用户对 **Feel Cool TV** 各个界面组件、页面状态、交互动作及物理按键的标准称呼与技术解释：

### 1. 页面与全局视图状态称呼
| 用户标准称呼 | 对应 DOM / 状态变量 | 详细解释与边界定义 |
| :--- | :--- | :--- |
| **主页面 / 主页** | `!isFullScreen && !isModalOpen && tvShutdownPhase === 'idle'` | 应用的核心浏览页面（包含侧边栏、中央大标题信息区、丸形菜单、横向卡片轨道以及剧集展开子视图），区别于**正式播放页面**、**密码页**、**设置页**与**电源选项页**。主页面下背景可以同时播放预览/背景视频流。 |
| **正式播放页面** | `isFullScreen === true` | 隐藏侧边栏（`#sidebar.fullscreen-hidden`，含左侧边栏与顶部侧边栏）、主内容卡片区（`#mainContent`）与背景遮罩（`#heroOverlay`），将视频画面（`#mac-bg-video`）完全铺满视口并开启声音的全屏沉浸播放视图。正式播放页面下严禁唤出顶部或左侧边栏；当视频处于暂停状态时，正式播放页面的视频标题跑马灯（`#iptvHudTitle`）限制滚动 10 轮后自动停止。 |
| **设置页面 / 设置页（多级全屏设置页面）** | `#settingsModal.active` (`isModalOpen === true`, `settingsCurrentLevel = 1 \| 2 \| 3`) | 点击侧边栏“设置”唤出的 Apple TV (tvOS) 深色毛玻璃风格三级全屏设置覆盖页（采用 `blur(78px) saturate(185%) brightness(0.82)` 多层环境光透出遮罩；列表滚动采用保留动量的临界阻尼 `rAF` 引擎与上下边缘渐隐遮罩）：<br>1. **一级页面**（`#settingsLevel1View`）：展示“媒体与账户设置”、“个性化设置”分类入口按钮（长按任意分类按钮或按 `F17` 显现“限制级内容设置”按钮），底部显示纯数字版本号 `4.7.0`；<br>2. **二级页面**（`#settingsLevel2View`）：展示当前分类下的设置项列表，填写性选项右侧显示 `>` SVG，非填写性选项右侧直接显示当前状态名；<br>3. **三级页面**（`#settingsLevel3View`）：针对单个填写性或账号连接设置项的独立全屏配置页。 |
| **密码页** | `#passwordModal.active` (`isModalOpen === true`) | 在侧边栏头像处长按（或按 `F17`）唤出的限制级空间解锁密码验证页面（采用与设置页同款的 Apple TV 深色高透毛玻璃遮罩，含右侧可展开的九键屏幕数字键盘 `#pwdNumpadPanel`），支持按 `Esc` 键直接退出。 |
| **电源选项页** | `#powerModal.active` (`isModalOpen === true`) | 点击侧边栏“电源”按钮唤出的 Apple TV 深色高透毛玻璃遮罩页，包含“关闭电源”、“重新启动”、“取消”三个选项。 |
| **关机遮罩 / 关闭电视页面** | `#tvShuttingDownModal` + `#tvBlackoutCurtain` (`tvShutdownPhase !== 'idle'`) | 按下 `F19` 键触发的睡眠关机过渡视图：先显示 5 秒“正在关闭电视.. / 请稍候。”模糊色彩遮罩，再切入 3 秒全黑屏，最后静默回到主页面。 |
| **窗口模式 / 全屏独占模式（全屏模式）** | `isAppInFullscreenMode()` (`macWindowFullscreenState`) | 指 macOS 应用程序窗口本身的窗口化状态与系统全屏独占状态。只要未进入**全屏独占模式**（即使在**窗口模式**下将窗口放大到最大），均允许鼠标指针正常出现；仅在**全屏独占模式**下严格隐藏鼠标指针（`html.app-cursor-hidden`）。任何情况下禁止使用 `Esc` 键切换窗口模式与全屏独占模式。 |
| **背景主题（波浪 / 光影）** | `appSettings.bgStyle` (`'wave'` \| `'light'`) + `#pspWaveCanvas` | 位于【设置 -> 个性化设置 -> 背景主题】的风格化动态背景切换项：**“波浪”** 为经典流光丝带波浪动态背景，**“光影”** 为无固定形状的柔和低亮度模糊光影在屏幕流动的动态背景；“背景配色方案”对“波浪”与“光影”均生效。 |

### 2. 核心界面组件称呼
| 用户标准称呼 | 对应 DOM 选择器 / 函数 | 详细解释与交互说明 |
| :--- | :--- | :--- |
| **焦点 / 选中样式图形** | `currentFocusEl` + `#sidebarFocusPill` / `.settings-focus-pill` | 当前被方向键选中的高亮可交互元素。在侧边栏与设置页中，焦点移动时直接将选中样式的胶囊图形平滑平移到目标位置。 |
| **交互音效 / 交互音效方案 / 交互音效音量** | `playUiSound()` (`appSettings.uiSound`, `appSettings.uiSoundStyle`, `appSettings.uiSoundVolume`) | 位于【设置 -> 个性化设置】的空间感交互反馈音效系统，支持在 **“深海流光”**（默认，平滑舒缓无点击感）、**“星云潮汐”**（空灵潮汐滑音无点击感）与 **“深邃脉冲”**（瞬态低频脉冲）三种方案间切换，支持开启/关闭及 `20%`–`100%` 音量大小调节，并在有视频有声播放时自动增强穿透力。 |
| **侧边栏（位置：左侧 / 顶部；材质：Liquid Glass 深色·浅色 / 传统模糊；方案：自动折叠 / 始终折叠 / 自动隐藏 / 始终隐藏 / 始终显示）** | `#sidebar` (`col === 0`, `appSettings.sidebarPosition = 'left' \| 'top'`, `appSettings.sidebarStyle = 'liquid' \| 'blur'`, `appSettings.liquidGlassTheme = 'dark' \| 'light'`, `appSettings.sidebarMode = 'auto' \| 'always' \| 'autohide' \| 'alwayshide'`) | 支持在【设置 -> 个性化设置】中切换 **“左侧”** 与 **“顶部”** 两种位置方案：<br>1. **顶部侧边栏（`顶部`，`.sidebar.top-bar`）**：遵循 Apple 官方 tvOS 长连贯药丸胶囊设计，支持 **“始终显示”** 与 **“始终隐藏”**（`alwayshide`）两种方案，进入正式播放页面时自动隐藏且正式播放页面不允许唤出；在“始终隐藏”方案下，仅按下菜单键（`F16`）才会唤出顶部侧边栏并将焦点放置在当前所在的导航项。<br>2. **左侧边栏（`左侧`）**：支持 **“Liquid Glass”**（原生支持 **“深色”** 与 **“浅色”** 两种外观模式）与 **“传统模糊”** 两种材质，以及 **“自动折叠”**、**“始终折叠”**、**“自动隐藏”**、**“始终隐藏”** 四种折叠方案。在 **“始终隐藏”** 模式下，侧边栏与下层页面彻底分层覆盖，绝不改变主页面元素位置，召唤侧边栏仅能使用菜单键（`F16`）。<br>3. 当焦点移动至侧边栏“设置”、“电源”或“头像”时，主页面标题和卡片仍停留在之前的内容上不做改变。 |
| **正在观看 / 现在观看（现在播放中）** | `#navNowPlayingItem` (`data-row="7" data-col="0"`) | 侧边栏的第一个功能导航项，**每次启动 APP 时焦点自动落在此项上**（“始终隐藏”模式下自动落在“现在观看/正在观看”第一个卡片上以保持侧边栏隐藏）。配备“很酷 TV+”、“有线电视”、“短视频”（及解锁后的“限制级”）独立丸形菜单分类，**选中的丸形菜单始终实时同步对应当前正在播放的内容**（`syncActiveNowWatchingTabToCurrentPlayback()`），且**每个分区独立享有 20 条观看历史记录额度**（`enforcePerSectionHistoryQuota()`），互不覆盖顶替。其中短视频内容在“正在观看/现在观看”板块中的大标题显示为 **`作者名 + 的视频`**（如 `@李子柒 的视频`），副标题显示为 **`短视频社区 · 媒体源`**（如 `短视频社区 · 抖音`、`短视频社区 · bilibili`）。 |
| **大标题 / 副标题 / 小标题 / 影视 Logo** | `#heroTitle` / `#heroSub` / `#heroBrand` (`updateHeroUI()`) | 主页面左上方信息展示区（`#heroText`）：<br>1. 当焦点位于侧边栏导航项（或尚未进入板块内第一个卡片）时，除“正在观看/现在观看”板块外，隐藏当前板块**副标题**（`#heroSub`）与**小标题**（`#heroBrand`），仅保留**大标题**（`#heroTitle`）；当焦点移动到板块内第一个卡片时副标题和小标题恢复显示；<br>2. 在**短视频**板块下，聚焦卡片时卡片上方**大标题**显示 `作者名 + 的视频`（如 `@李子柒 的视频`），**副标题**显示 `来自 + 媒体源及分区`（如 `来自 抖音精选`、`来自 bilibili热门`）；<br>3. 在**很酷 TV+** 板块下，侧边栏与丸形菜单状态的大标题统一显示为 `很酷 TV+`（不再附加“剧集”或“电影”字样），聚焦具体影视卡片时优先渲染本地缓存的 TMDB 官方透明影视 Logo（`.tmdb-logo-stage`）。 |
| **丸形菜单、短视频搜索框与短视频刷新指示器** | `#seasonBar`、`.season-pill`、`#streamSearchPill`、`#streamSearchInput`、`#streamRefreshIndicator` | 位于大标题下方、卡片轨道上方的横向胶囊形分类/选季菜单栏，非聚焦态采用原生 Liquid Glass 材质。一级丸形菜单默认显示当前选中的子菜单项名（如 `抖音精选`、`bilibili 推荐`）；展开二级丸形菜单时，首项（`data-col="1"`）为与相邻丸形按钮等高的**搜索框**（`#streamSearchPill`），聚焦时自动进入输入状态并实时屏蔽标点符号，激活时按返回键（`Escape` / `Backspace`）仅用于退格（退格到最左侧为空时亦保持聚焦不失焦，需使用方向键移出搜索框），按确认键触发搜索；左侧配备无底框刷新指示器 `#streamRefreshIndicator`，**只要触发刷新时，刷新符号自动旋转**（`.spinning`）。 |
| **卡片 / 第一个卡片 / 第二个卡片 / 暂无视频内容卡片** | `.media-item.focusable` (`#mainTrack` 内，`col >= 1`) | 主页面下方横向滚动轨道中的 16:9 媒体海报卡片。短视频卡片下方的视频标题过长时在聚焦态下自动开启跑马灯滚动显示（`.card-title.single-line-marquee`）。无内容时的**“暂无视频内容”卡片**采用与刷新按钮卡片一致的半透明毛玻璃加居中 `+` 符号 SVG（`.empty-card-wrapper`）。 |
| **进度条与正式播放页播放按钮（Liquid Glass 播放按钮）** | `#iptvFullscreenHud` / `#fsProgressWrap` / `#fsPlayStateBtn` (`.fs-play-state-btn`) | 正式播放页面底部的半透明黑色渐变信息遮罩与播放进度控制条。左侧播放/暂停状态按钮（`#fsPlayStateBtn`，`42×42px`）采用 macOS 27 原生 Liquid Glass 圆形液态玻璃材质设计。 |
| **点赞符号与收藏符号（点赞与收藏状态指示器）** | `#fsStreamActionIndicators` (`#fsStreamLikeInd` / `#fsStreamCollectInd`) | **仅在短视频（`sourceRow === 3`）正式播放页面下**显示的互动状态图标组，位于底部视频标题与作者副标题右侧居右显示（间距 `28px`）；很酷 TV+ 与有线电视正式播放页面严格隐藏该指示器。默认白色，点赞激活为品红色（`#ff007f`），收藏激活为黄色（`#ffd60a`）。 |
| **亮度条 / 亮度滑杆 / 亮度符号** | `#tvBrightnessHud` / `#tvBrightnessIconBtn` | 由 **F20 键**触发的底部半透明黑色渐变电视亮度控制条，左侧直接显示无按钮容器的**亮度符号**（`.tv-brightness-icon`），右侧为实时响应用户左右键（**`±10%` 步长**）操作的**亮度滑杆**与 `0-100` 纯数字亮度值。 |
| **选集菜单 / 选集列表** | `#fsEpisodePickerHud` (`.fs-ep-item-btn` / `.fs-ep-season-arrow`) | 剧集类视频在**正式播放页面**下，通过按下方向键（`ArrowDown`）或菜单键（`F16`）在画面底部唤出的选集列表遮罩，选集按钮与季切换箭头采用原生 Liquid Glass 材质设计。 |
| **短视频选项 / 短视频选项菜单 / 只看作者** | `#fsStreamMenuHud` / `#fsStreamAuthorOnlyBtn` | 短视频在**正式播放页面**下，通过按下菜单键（`F16`）在画面底部唤出的短视频专用选项遮罩，包含带对勾 SVG 激活状态指示器的 **“只看作者”** Liquid Glass 丸形按钮。 |
| **整点/半点时钟** | `#topRightClock` (`.top-right-clock`) | 在整点与半点的前一分钟至整点/半点结束期间（`:29`–`:30`、`:59`–`:00`），于屏幕右上角浮现的无底框柔和扩散阴影数字时钟。 |
| **气象信息 / 气象速报** | `#jpWeatherTelop` | 屏幕顶部居中弹出的即时灾害预警、降水与高湿提示横幅（带彩色底部下划线）。 |

### 3. 标准键盘与遥控器功能键称呼
| 用户标准称呼 | 对应键盘事件 (`key` / `keyCode`) | 功能定义与触发行为 |
| :--- | :--- | :--- |
| **主页键** | 标准键盘上的 **`F18` 功能键** (`keyCode: 129`) | 全局主页与正式播放页面双向切换键：<br>1. 在**正式播放页面**按下：先将“正在观看”选中的丸形菜单同步至当前播放内容分区，再退回**主页面**并将焦点移至“正在观看”，背景丝滑继续播放且绝不误聚焦其他分区卡片打断视频；<br>2. 在**主页面**只要存在活动视频流（无论正在播放还是**处于暂停状态**）时按下：立刻自如切回**正式播放页面**，不打断播放且不丢失进度；<br>3. 在密码页、设置页、电源选项页按下：关闭弹窗回到**主页面**并将焦点移至“正在观看 / 现在观看”。 |
| **上下文菜单键 / 菜单键** | 标准键盘上的 **`F16` 功能键** (`keyCode: 127` / `ContextMenu`) | 全局应用级菜单键（**严禁触发长按 `Return` 键动作**）：<br>1. 在剧集类视频**正式播放页面**按下：唤出/收起底部**选集菜单**（`#fsEpisodePickerHud`）；<br>2. 在流媒体视频**正式播放页面**按下：唤出/收起底部**流媒体选项菜单**（`#fsStreamMenuHud`）；<br>3. 在其余视频正式播放页面按下：显隐底部**进度条**；<br>4. 在**主页面卡片或丸形菜单区**按下：唤出侧边栏（为“始终隐藏”模式下唯一召唤左侧边栏的按键）并将焦点移动至侧边栏中的当前导航项；<br>5. 在**侧边栏**按下：收起侧边栏（或从顶部侧边栏移回卡片）并将焦点妥善放置在当前板块对应的卡片位置。 |
| **长按替代键** | 标准键盘上的 **`F17` 功能键** (`keyCode: 128`) | **唯一可替代长按 `Return` 键功能的快捷键**：<br>1. 在**流媒体正式播放页面**单击 `F17`：直接触发当前流媒体视频的**收藏/取消收藏**功能；<br>2. 在主页面或弹窗单击 `F17`：等效触发当前焦点元素的“长按确认键（550ms）”操作（如长按头像打开密码页/锁定限制级、长按“正在观看”停止背景播放、长按任意丸形菜单刷新当前内容、长按密码框唤出屏幕数字键盘、长按设置页一级分类按钮展开“限制级内容设置”）。 |
| **关机键 / 睡眠键** | 标准键盘上的 **`F19` 功能键** (`keyCode: 130`) | 触发模拟关闭电视睡眠流程：强制暂停当前播放内容 -> 显示“正在关闭电视.. / 请稍候。”模糊色彩遮罩 5 秒 -> 全黑屏 3 秒 -> 静默回到主页面并将焦点移至“现在观看”。 |
| **亮度键** | 标准键盘上的 **`F20` 功能键** (`keyCode: 131`) | 唤出底部**亮度条**（`#tvBrightnessHud`），并通过左右方向键以 **`±10%` 步进**实时调节电视亮度滑杆（前端显示 `0-100` 纯数字），自动替换并隐藏视频播放进度条。 |
| **返回键** | **`Esc` 键** (`Escape`, `keyCode: 27`) / **`Backspace` 键**（以及位于第一个卡片或设置二级页面时的左方向键 `ArrowLeft`） | 1. **任何情况下均禁止使用 `Esc` 键切换窗口模式与全屏独占模式**；<br>2. 在**正式播放页面**按下：退回**主页面**（保持背景视频丝滑播放且不丢失进度）；<br>3. 在多级设置页按下：从三级页面返回二级页面、从二级页面返回一级页面、从一级页面关闭设置回到主页面；在密码页/电源选项页/选集菜单/流媒体选项菜单/亮度条按下：关闭当前层；<br>4. 在**“始终隐藏”模式下已召唤出左侧边栏时**，按下返回键（`Esc` / `Backspace`）或左右方向键（`ArrowLeft` / `ArrowRight`）直接隐藏左侧边栏并回到当前板块卡片；在非“始终隐藏”模式下，位于第一个卡片（`col === 1`）或“自动隐藏”/“顶部侧边栏”模式下按返回键，直接将焦点移至侧边栏当前导航项；<br>5. 在**非“正在观看/现在观看”的侧边栏导航项**（`col === 0 && row !== 7`）按下 `Esc`：将焦点移动到“正在观看/现在观看”；<br>6. 在**“正在观看”**按下 `Esc`：丝滑切换到**正式播放页面**；在**“现在观看”**按下 `Esc`：不作任何反应。 |
| **上下键 / 左右键** | `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` | 十字方向导航键。注意：**IPTV（有线电视）与流媒体正式播放页面下，下键（`ArrowDown`）统一对应上一个频道/视频，上键（`ArrowUp`）统一对应下一个频道/视频（开启“只看作者”时在该作者作品列表内环形切换上一条/下一条视频）**；**剧集类视频正式播放页面禁止使用上下键切换上一集/下一集，按下键（`ArrowDown`）用于唤出选集菜单**；亮度条激活时左右键按 `±10%` 实时调节亮度，其余正式播放状态下左右键调节视频进度条。 |
| **确认键 / 回车键** | `Enter` / `Return`（正式播放页面下含空格键 `Space`） | 1. 在**流媒体正式播放页面**下（仅当在全屏状态下按下 `Return` 时生效，从卡片按 `Return` 进入全屏后松开按键不会误触发暂停）：**单击 `Return`** 暂停/继续播放，**双击 `Return`** 触发点赞/取消点赞（品红色指示器高亮），**长按 `Return`（满 `550ms`）** 触发收藏/取消收藏（黄色指示器高亮）；<br>2. 在其余场景下：短按触发当前焦点按钮/卡片或切换正式播放暂停/继续；在支持长按的元素上按住满 `550ms` 触发长按动作。 |

