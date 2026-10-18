# Feel Cool TV 技术规范与架构维护手册 (v0.1.5 持续演进版)

本文档面向参与 **Feel Cool TV (`cooltv-mac`)** 后续迭代的开发人员，系统梳理产品的核心调性、视觉与交互设计规范、界面文案措辞准则、全局快捷键与遥控映射体系、**用户对各组件与按键的标准称呼词典**，并重点复盘前端（WebKit / HTML5 / CSS3 / HLS.js）与后端（Tauri v2 / Rust / FFmpeg / Rouille）在历次迭代中遇到的疑难问题、底层成因及最终工程解决方案。

> **版本同步强制规范**：每次完成迭代任务后，必须同步更新产品版本号（当前版本：**`0.1.5`**），涉及位置包括：
> 1. `src/index.html` -> `#settingsVersionText`（仅显示纯数字版本号如 `0.1.5`）；
> 2. `src-tauri/tauri.conf.json` -> `"version"`；
> 3. `src-tauri/Cargo.toml` -> `[package] version`；
> 4. `package.json` -> `"version"`；
> 5. `TECHNICAL_SPEC.md` 标题与正文版本记录。
>
> **组件称呼记录强制规范（Round 20 任务 14 新增）**：每次任务结束后，必须在本文档第五章 **“五、组件、页面与快捷键标准称呼词典”** 中详细记录并持续维护用户对每个组件、页面、状态与按键的称呼及对应技术解释。

---

## 一、产品调性与核心定位

1. **10-Foot 客厅大屏沉浸体验（tvOS × PSP XMB 融合美学）**
   - 产品定位于 macOS 全屏客厅媒体中心，融合 **Apple TV (tvOS)** 的毛玻璃层级、呼吸感字重与 **Sony PlayStation / PSP XMB（交叉媒体栏）** 的丝带流光背景及十字轴焦点导航体验。
   - **背景即正式播放舞台**：任何触发了后台视频流预览的画面均视为正在进行的正式播放。只要用户未聚焦到另一个可播放媒体卡片或主动长按停止，焦点在导航栏、丸形菜单或设置项之间移动时**绝不打断当前背景播放**。
   - **全屏模式隐藏鼠标指针、窗口模式允许鼠标指针可见（Round 18 新增，Round 20 演进）**：
     - 当应用处于 **全屏模式** 时，严格执行鼠标指针不可见（`html.app-cursor-hidden`，全局 `cursor: none !important`）；
     - 当应用处于 **窗口模式** 时，移除 `app-cursor-hidden`，允许鼠标指针正常可见（`syncAppCursorVisibility()` 动态监听窗口全屏状态与尺寸变化）。
2. **全键盘/遥控器优先，二维十字轴精准方向键导航**
   - 所有界面元素均可通过方向键（`ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight`）、确认键（`Enter`）、返回键（`Escape` / `Backspace`）及全局功能键（`F16` / `F17` / `F18` / `F19` / `F20`）完成闭环导航。
   - **丸形菜单（`#seasonBar`）导航与流媒体单行横向滚动遮罩规范（Round 18 & Round 19 演进）**：
     - 流媒体二级分类菜单（`douyinPillsExpanded`）强制只显示一行（`.season-bar.single-line-mode`），置于 `.season-pills-scroll-viewport` 内，左右两侧采用跑马灯同款渐变透明遮罩（`linear-gradient(to right, transparent 0px, black 26px, black 93%, transparent 100%)`），当用户左右移动焦点时通过 `scrollSeasonPillIntoView()` 平滑横向滚动菜单；
     - 流媒体左侧刷新元素改为无圆形容器的纯刷新符号状态指示器（`.stream-refresh-indicator`），不再作为可聚焦按钮；**用户长按任何一个丸形菜单项（或聚焦任意丸形菜单时按 `F17`）即可触发刷新当前内容**，刷新期间刷新符号旋转并展开显示“正在刷新”；
     - 非单行模式下的多行丸形菜单仍通过 `findSeasonPillInAdjacentRow()` 自动按视觉纵坐标聚类识别多行结构，支持 `ArrowUp` / `ArrowDown` 跨行直达。
   - **第一个卡片返回左侧边栏规范（Round 20 任务 7 新增）**：
     - 任何情况下，当**焦点**位于当前轨道的**第一个卡片**（`col === 1, row >= 1`）时，按下**返回键**（`Escape`、`Backspace` 或向左方向键 `ArrowLeft`）直接返回到**左侧边栏**对应导航项（`col === 0`），绝不返回到上方的**丸形菜单**（`#seasonBar`）。
   - **“正在观看”按钮直达正式播放页面规范（Round 20 任务 6 新增）**：
     - 任何情况下，只要点击或短按确认左侧边栏的 **“正在观看 / 现在观看”** 按钮（`#navNowPlayingItem`，`row === 7, col === 0`），立即进入**正式播放页面**（`enterNowPlayingFullscreen()`）；
     - 如需选取其他历史记录卡片，必须使用**右键**（`ArrowRight`）将焦点从“正在观看”移动到右侧卡片轨道。
   - **设置页与弹窗严格的“上下键跨行、左右键同行”二维导航规范（Round 18 ~ Round 20 演进）**：
     - 在设置页（`#settingsModal`）与密码页（`#passwordModal`）中，**`ArrowUp` / `ArrowDown` 只能用于在不同行之间上下移动，绝不横向切换同行元素；`ArrowLeft` / `ArrowRight` 只能用于在同一行内的并排控件或分段控制器（`.seg-control`）选项之间左右移动**；
     - **密码页（`#passwordModal`）、电源选项页（`#powerModal`，Round 20 任务 4 强化）与设置页（`#settingsModal`）均全面支持按 `Escape` 键直接关闭并返回主页面**（即使电源选项页是在正式播放页面上方唤出，按 `Escape` 后也会一并退回主页面）。
   - **全局功能快捷键规范（Round 19 & Round 20 演进）**：
     - **`F20`（亮度调节键，Round 20 任务 11 新增）**：按下 `F20` 触发底部**亮度条**（`#tvBrightnessHud`），使用 `ArrowLeft` / `ArrowRight` 每次按 `±20%` 调节电视亮度（底层调用 `/Applications/BetterDisplay.app/Contents/MacOS/BetterDisplay`），前端以 `0-100` 纯数字显示当前亮度值；**亮度条与视频播放进度条（`#iptvFullscreenHud`）同时只能二选一显示，二者严格相互替换，绝不同时出现或覆盖显示**。
     - **`F19`（模拟关闭电视睡眠流程键，Round 19 升级）**：按下 `F19` 不再唤出电源选项页，改为立即触发**强制暂停当前播放内容**，展示与电源选项页同款的模糊色彩遮罩层（`#tvShuttingDownModal`），画面中央以与电源页完全一致的大标题（`38px`）显示 **“正在关闭电视..”**、小标题（`16px`）显示 **“请稍候。”**；计时 **5 秒**后切入全黑屏（`#tvBlackoutCurtain`），黑屏持续 **3 秒**后静默回到主页面，并将焦点移动到 **“现在观看”**（`#navNowPlayingItem`）。
     - **`F18`（主页键，Round 19 & Round 20 任务 2、3 升级）**：
       - 只要当前位于**主页面**（非正式播放页面、非密码页、非设置页、非电源选项页）且**背景有正在播放的内容**，按下主页键（`F18`）立即进入**正式播放页面**，丝滑继续背景内容的当前实时播放进度，绝不打断播放或回退到离开正式播放页面时的旧进度；
       - 在**正式播放页面**按下主页键（`F18`），切换到**主页面**并将焦点移动到“正在观看 / 现在观看”（`#navNowPlayingItem`），背景继续播放并实时同步观看进度；
       - 在密码页、设置页、电源选项页或关机遮罩页按下主页键（`F18`），立即关闭当前弹窗/遮罩，切换到主页面并将焦点移动到“正在观看 / 现在观看”。
     - **`F17`（全局长按替代键）**：当用户不便长按回车键时，单击一次 `F17` 即可立即触发当前焦点元素的长按动作（包括在任意丸形菜单上按 `F17` 触发刷新当前内容）。
     - **`F16`（全局上下文菜单键 & 本地剧集选集呼出键，Round 18 & Round 20 任务 9 演进）**：
       - 全局拦截并禁用 macOS WebKit 底层原生右键菜单；
       - **剧集类视频正式播放页面禁止使用上下键（`ArrowUp` / `ArrowDown`）切换上一集/下一集**；剧集类视频正式播放时允许使用**下键（`ArrowDown`）**直接唤出底部**选集菜单**（`#fsEpisodePickerHud`），同时保留 `F16` 键唤出/收起选集菜单功能（流媒体正式播放不受此影响，仍保持上下键切换视频）。
3. **双空间隐私架构与屏幕数字键盘（常规空间 + 隐藏限制级空间）**
   - 默认界面与设置页对限制级功能实现**零视觉暴露**。
   - 在密码验证页长按密码输入框（或按 `F17`）可唤出/收起右侧大九键屏幕数字键盘（`#pwdNumpadPanel`，含 Apple 规范 `delete.left` 退格键）。
   - **“正在观看 / 现在观看”丸形菜单分组、限制级隐私隔离与清空历史记录（Round 19 & Round 20 任务 1 演进）**：
     - “正在观看 / 现在观看”页面配备丸形菜单（`#seasonBar`），将卡片按 **全部**、**很酷 TV+**、**有线电视**、**流媒体**、**限制级** 分组；
     - **限制级内容严禁出现在“全部”分组中**（常规历史存于 `fc_watch_history_v1`，限制级历史独立存于 `fc_watch_history_porn_v1`）；且**只有当限制级内容解锁后（`pornUnlocked === true`），才会在“正在观看 / 现在观看”丸形菜单中出现“限制级”选项**；
     - 设置页面新增 **“清空观看历史记录”** 按钮（`#btnClearWatchHistory`），支持一键清空全部常规与限制级历史观看记录及进度缓存。

---

## 二、视觉与设计规范

### 1. 空间布局、侧边栏折叠方案与全局自适应缩放体系 (`autoScaleUI`)
- **基准视口与左侧边栏折叠方案（Round 18 新增）**：
  - 左侧边栏（`#sidebar`）固定定位于 `left: 20px; top: 20px; bottom: 20px;`，展开宽度 `205px`，折叠宽度 `68px`（`.sidebar.collapsed`）。
  - 在偏好设置页新增 **“左侧边栏折叠方案”**（`#btnToggleSidebarMode`，持久化于 `localStorage.fc_sidebar_mode`，默认 **`自动折叠`**）：
    - **`自动折叠` (`auto`)**：焦点在左侧边栏（`col === 0`）时展开，焦点移入右侧内容区（`col >= 1`）时自动折叠为胶囊态。
    - **`始终折叠` (`always`)**：左侧边栏始终保持 `.collapsed` 紧凑图标胶囊样式（主内容区始终 `.expanded`），同时用户依然可以 100% 正常移动焦点至左侧边栏各图标项进行导航、单击或长按操作。
- **全局缩放预设 (`UI_SCALE_PRESETS`)**：
  - 提供 `100%` ~ `200%` 六档缩放，严禁对 `body`、`#macVideo` 或 `#pspWaveCanvas` 应用 `zoom`。

### 2. 色彩系统、柔化底部黑色渐变遮罩与全屏遮罩色调联动
- **PSP XMB 动态丝带背景 (`PSP_BG_THEMES`)**：内置 6 套低饱和度深色主题及 `随机 random` 模式。
- **全屏播放进度条、选集菜单与亮度条柔化黑色透明渐变遮罩（Round 18 & Round 20 任务 11 升级）**：
  - 全屏播放底部信息与进度条遮罩（`.iptv-fullscreen-hud`）、本地剧集底部选集遮罩（`.fs-episode-picker-hud`）以及电视亮度调节遮罩（`.tv-brightness-hud`）统一采用柔化透明度的黑色渐变遮罩：
    `background: linear-gradient(to top, rgba(0, 0, 0, 0.72) 0%, rgba(0, 0, 0, 0.42) 54%, rgba(0, 0, 0, 0) 100%);`
  - **亮度条（`#tvBrightnessHud`）与播放进度条（`#iptvFullscreenHud`）互斥替换原则**：二者同时只能二选一显示，唤出亮度条时立即关闭进度条与选集菜单，唤出进度条或选集菜单时立即关闭亮度条。

### 3. 品牌图片 Logo 规范与字体规范 (Round 18 ~ Round 20 任务 12、13 更新)
- **系统主字体栈**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Helvetica Neue", sans-serif`。
- **“很酷 TV+”大标题专用图片 Logo（Round 20 任务 12 新增）**：
  - 当主页面中央信息区**大标题**（`#heroTitle`）为 `"很酷 TV+"` 时，使用指定透明白色图片 Logo（`./assets/cooltv_plus.png`），通过 `renderHeroLogoElement(heroTitleEl, title, './assets/cooltv_plus.png')` 按与 TMDB 剧集 Logo 完全一致的舞台容器（`.tmdb-logo-stage`）、投影与入场动画渲染；
  - **严格仅改动大标题这一处**，左侧边栏内的导航项（`#sidebar`）以及其他任何位置的 `"很酷 TV+"` 文本均保持不变。
- **`bilibili` 全局图片 Logo 替换与空格规范（Round 20 任务 13 新增）**：
  - 界面中所有 `bilibili` 字样统一使用透明白色图片 Logo（`<img class="inline-bili-logo" src="./assets/bilibili.png" alt="bilibili">`）代替；
  - **丸形菜单内（`isPillMenu === true`）**：丸形菜单内的名称视为整体，因此 `bilibili` 图片 Logo 与后面的字符（如 `热门`、`推荐`、`动画` 等）**不加空格**；
  - **其余所有场景（`isPillMenu === false`）**：`bilibili` 图片 Logo 与前后相邻的其他非空白字符之间**必须用空格隔开**（如设置页 `登录 bilibili`、卡片角标与副标题等）。
- **抖音美好体 (`DouyinMeihaoti` / `.douyin-font`)**：单独出现时不加空格，嵌入其他文字中时通过 `formatDouyinBrandTextHtml()` 在两侧自动加空格。
- **气象速报字体与字号规范 (`#jpWeatherTelop`，Round 18 更新)**：
  - 使用标准系统无衬线字体栈；主行字号 `33px`，次行发布机构字号 `27px`。

---

## 三、用户交互内容的文字措辞规范

1. **严禁出现按键括号提示或机械操作说明**
2. **设置页文案与层级命名规范（Round 20 任务 1、10、13 更新）**
   - 页面主标题：**`偏好设置`**，副标题：**`影视库资源，流媒体账户，显示和个性化设置。`**
   - 分区小标题：**`媒体与账户设置`**、**`个性化设置`**、**`限制级内容设置`**（默认隐藏）
   - 设置项名称：
     - `数据同步`、`影视库资源位置`、`EPG 电子节目单来源`、`流媒体账号连接`（`登录 抖音`、`登录 bilibili`［含图片 Logo］）
     - `气象信息定位`（`自动定位` / `手动输入`）、`所在地位置`（`城市`、`区`、`街道`）
     - `影片评分来源`、`背景配色方案`、`用户界面显示比例`、`左侧边栏折叠方案`（`自动折叠` / `始终折叠`）、`预览时开启声音`、`音量均衡`
     - **`系统 VPN`（`开启` / `关闭`，直接触发 macOS 系统 VPN 开关）**
     - **`观看历史记录`（`清空观看历史记录` 按钮）**
     - `限制级媒体资源位置`、`限制级内容密码`
   - 页面最底部版本号：**只显示纯版本号数字**（当前为 `0.1.4`）。

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

---

## 五、组件、页面与快捷键标准称呼词典（任务 14 长期维护规范）

为确保后续每一次迭代指令与代码实现 100% 精准对齐，本章详细记录用户对 **Feel Cool TV** 各个界面组件、页面状态、交互动作及物理按键的标准称呼与技术解释：

### 1. 页面与全局视图状态称呼
| 用户标准称呼 | 对应 DOM / 状态变量 | 详细解释与边界定义 |
| :--- | :--- | :--- |
| **主页面 / 主页** | `!isFullScreen && !isModalOpen && tvShutdownPhase === 'idle'` | 应用的核心浏览页面（包含左侧边栏、中央大标题信息区、丸形菜单、横向卡片轨道以及剧集展开子视图），区别于**正式播放页面**、**密码页**、**设置页**与**电源选项页**。主页面下背景可以同时播放预览/背景视频流。 |
| **正式播放页面** | `isFullScreen === true` | 隐藏左侧边栏（`#sidebar`）、主内容卡片区（`#mainContent`）与背景遮罩（`#heroOverlay`），将视频画面（`#mac-bg-video`）完全铺满视口并开启声音的全屏沉浸播放视图。 |
| **设置页面 / 设置页** | `#settingsModal.active` (`isModalOpen === true`) | 点击左侧边栏“设置”唤出的全屏毛玻璃偏好设置覆盖页（页面主标题为“系统与媒体库设置”）。 |
| **密码页** | `#passwordModal.active` (`isModalOpen === true`) | 在左侧边栏顶部头像处长按（或按 `F17`）唤出的限制级空间解锁密码验证页面（含右侧可展开的九键屏幕数字键盘 `#pwdNumpadPanel`），支持按 `Esc` 键直接退出。 |
| **电源选项页** | `#powerModal.active` (`isModalOpen === true`) | 点击左侧边栏底部“电源”按钮唤出的全屏模糊色彩遮罩页，包含“关闭电源”、“重新启动”、“取消”三个选项。 |
| **关机遮罩 / 关闭电视页面** | `#tvShuttingDownModal` + `#tvBlackoutCurtain` (`tvShutdownPhase !== 'idle'`) | 按下 `F19` 键触发的睡眠关机过渡视图：先显示 5 秒“正在关闭电视.. / 请稍候。”模糊色彩遮罩，再切入 3 秒全黑屏，最后静默回到主页面。 |
| **窗口模式 / 全屏模式** | `isAppInFullscreenMode()` (`macWindowFullscreenState`) | 指 macOS 应用程序窗口本身的窗口化状态与系统全屏状态。**窗口模式**下允许鼠标指针可见；**全屏模式**下严格执行鼠标指针不可见（`html.app-cursor-hidden`）。 |

### 2. 核心界面组件称呼
| 用户标准称呼 | 对应 DOM 选择器 / 函数 | 详细解释与交互说明 |
| :--- | :--- | :--- |
| **焦点** | `currentFocusEl`（主界面 `.focused`）/ `modalFocusList[modalFocusIdx]`（弹窗 `.modal-focused`） | 当前被方向键选中的高亮可交互元素（带有白色高光描边、放大动效或胶囊高亮态）。键盘方向键、确认键与长按动作均作用于当前焦点所在元素。 |
| **左侧边栏 / 侧边栏** | `#sidebar` (`col === 0`) | 位于主页面左侧的纵向毛玻璃导航栏，包含顶部用户头像（`row = -1`）、**正在观看 / 现在观看按钮**（`row = 7`）、**很酷 TV+**（`row = 1`）、**有线电视**（`row = 2`）、**流媒体**（`row = 3`）、解锁后显示的**限制级**（`row = 4`），以及底部的**设置**（`row = 5`）和**电源**（`row = 6`）。 |
| **正在观看 / 现在观看** | `#navNowPlayingItem` (`data-row="7" data-col="0"`) | 左侧边栏的第一个功能导航项。当背景有视频正在播放（未暂停）时显示动态三竖线波形图标与“正在观看”；默认或暂停时显示圆形播放图标与“现在观看”。点击或短按确认该按钮立刻进入正式播放页面；**当焦点位于“正在观看”时按返回键（`Esc`）同样切换到正式播放页面；当焦点位于“现在观看”时按返回键（`Esc`）不作任何反应**。 |
| **大标题 / 流媒体标题 / 影视 Logo** | `#heroTitle` (`updateHeroUI()`) | 主页面左上方/中央信息展示区（`#heroText`）的主标题区域。当显示 `"抖音"`、`"bilibili"` 或 `"很酷 TV+"` 时均采用流媒体标题内联显示方式（其中 `bilibili` 与 `很酷 TV+` 使用四周无留白的专属内联图片 Logo）；当显示剧集/电影时优先渲染缩小 20% 后的 TMDB 官方透明影视 Logo（`.tmdb-logo-stage`）；当显示超长流媒体视频标题时启用双节点乒乓无闪烁单行跑马灯。 |
| **丸形菜单** | `#seasonBar` 及其子项 `.season-pill` (`data-row="0"`) | 位于大标题下方、卡片轨道上方的横向胶囊形分类/选季菜单栏。出现在“正在观看”、“很酷 TV+”剧集展开态、“流媒体”（抖音 / bilibili 一级与二级分类，其中 **bilibili 丸形菜单内显示为 `"bilibili XX"` 即在 bilibili Logo 与分类名之间带空格**）以及“限制级”。长按任意一个丸形菜单项（或按 `F17`）可触发刷新当前板块内容。 |
| **卡片 / 第一个卡片** | `.media-item.focusable` (`#mainTrack` 内，`col >= 1`) | 主页面下方横向滚动轨道（`#mainTrack`）中的 16:9 媒体海报卡片。其中 **`col === 1` 的卡片称为“第一个卡片”**：**任何情况下当焦点位于第一个卡片时，按返回键（`Esc`、`Backspace` 或左方向键 `ArrowLeft`）直接返回左侧边栏，不返回到丸形菜单**。 |
| **进度条（视频播放进度条）** | `#iptvFullscreenHud` / `#fsProgressWrap` | 正式播放页面底部的半透明黑色渐变信息遮罩与播放进度控制条（包含播放/暂停图标、跑马灯标题、副标题、进度滑块与当前时间/总时长）。左右键可快进/快退，与**亮度条**严格互斥替换显示。 |
| **亮度条 / 亮度滑杆 / 亮度符号** | `#tvBrightnessHud` / `#tvBrightnessIconBtn` | 由 **F20 键**触发的底部半透明黑色渐变电视亮度控制条，左侧直接显示无按钮容器的**亮度符号**（`.tv-brightness-icon`），右侧为实时响应用户左右键（`±20%`）操作的**亮度滑杆**与 `0-100` 纯数字亮度值，与视频播放进度条严格二选一互斥替换显示。 |
| **选集菜单** | `#fsEpisodePickerHud` | 剧集类视频在**正式播放页面**下，通过按下方向键（`ArrowDown`）或上下文菜单键（`F16`）在画面底部唤出的选集列表遮罩，包含左右两端 SVG `<` / `>` 季切换按钮与当前季各集丸形按钮。 |
| **整点/半点时钟** | `#topRightClock` (`.top-right-clock`) | 在整点与半点的前一分钟至整点/半点结束期间（`:29`–`:30`、`:59`–`:00`），于屏幕右上角浮现的无底框柔和扩散阴影数字时钟。 |
| **气象信息 / 气象速报** | `#jpWeatherTelop` | 屏幕顶部居中弹出的即时灾害预警、降水与高湿提示横幅（带彩色底部下划线）。 |

### 3. 标准键盘与遥控器功能键称呼
| 用户标准称呼 | 对应键盘事件 (`key` / `keyCode`) | 功能定义与触发行为 |
| :--- | :--- | :--- |
| **主页键** | 标准键盘上的 **`F18` 功能键** (`keyCode: 129`) | 全局主页与正式播放页面双向切换键：<br>1. 在**正式播放页面**按下：退回**主页面**并将焦点移至“正在观看”，背景丝滑继续播放且不丢失进度；<br>2. 在**主页面**且背景有正在播放内容时按下：立刻丝滑进入**正式播放页面**，不打断播放；<br>3. 在密码页、设置页、电源选项页按下：关闭弹窗回到**主页面**并将焦点移至“正在观看 / 现在观看”。 |
| **上下文菜单键** | 标准键盘上的 **`F16` 功能键** (`keyCode: 127` / `ContextMenu`) | 全局应用级上下文菜单键：在剧集类视频**正式播放页面**按下时唤出/收起底部**选集菜单**；在普通视频正式播放页面按下时显隐底部**进度条**；在主页面或弹窗按下时触发当前焦点元素的长按动作。 |
| **长按替代键** | 标准键盘上的 **`F17` 功能键** (`keyCode: 128`) | 单击一次即可等效触发当前焦点元素的“长按确认键（550ms）”操作（如长按头像打开密码页/锁定限制级、长按“正在观看”停止背景播放、长按任意丸形菜单刷新当前内容、长按密码框唤出屏幕数字键盘、长按设置页“保存设置”展开限制级设置）。 |
| **关机键 / 睡眠键** | 标准键盘上的 **`F19` 功能键** (`keyCode: 130`) | 触发模拟关闭电视睡眠流程：强制暂停当前播放内容 -> 显示“正在关闭电视.. / 请稍候。”模糊色彩遮罩 5 秒 -> 全黑屏 3 秒 -> 静默回到主页面并将焦点移至“现在观看”。 |
| **亮度键** | 标准键盘上的 **`F20` 功能键** (`keyCode: 131`) | 唤出底部**亮度条**（`#tvBrightnessHud`），并通过左右方向键以 `±20%` 步进实时调节电视亮度滑杆（前端显示 `0-100` 纯数字），自动替换并隐藏视频播放进度条。 |
| **返回键** | **`Esc` 键** (`Escape`, `keyCode: 27`) / **`Backspace` 键**（以及位于第一个卡片时的左方向键 `ArrowLeft`） | 1. 在**正式播放页面**按下：退回**主页面**（保持背景视频丝滑播放且不丢失进度）；<br>2. 在设置页/密码页/电源选项页/选集菜单/亮度条按下：关闭当前层回到主页面；<br>3. 在**第一个卡片**（`col === 1`）按下：直接返回左侧边栏对应导航项；<br>4. 在**非“正在观看/现在观看”的侧边栏导航项**（`col === 0 && row !== 7`）按下 `Esc`：将焦点移动到“正在观看/现在观看”；<br>5. 在**“正在观看”**按下 `Esc`：丝滑切换到**正式播放页面**；在**“现在观看”**按下 `Esc`：不作任何反应。 |
| **上下键 / 左右键** | `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` | 十字方向导航键。注意：**剧集类视频正式播放页面禁止使用上下键切换上一集/下一集，按下键（`ArrowDown`）用于唤出选集菜单**；流媒体正式播放页面保持使用上下键切换上一个/下一个视频；亮度条激活时左右键实时调节亮度，其余正式播放状态下左右键调节视频进度条。 |
| **确认键 / 回车键** | `Enter`（正式播放页面下含空格键 `Space`） | 短按触发当前焦点按钮/卡片或切换正式播放暂停/继续；在支持长按的元素上按住满 `550ms` 触发长按动作。 |

