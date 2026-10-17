# Feel Cool TV 技术规范与架构维护手册 (v0.1.2 持续演进版)

本文档面向参与 **Feel Cool TV (`cooltv-mac`)** 后续迭代的开发人员，系统梳理产品的核心调性、视觉与交互设计规范、界面文案措辞准则、全局快捷键与遥控映射体系，并重点复盘前端（WebKit / HTML5 / CSS3 / HLS.js）与后端（Tauri v2 / Rust / FFmpeg / Rouille）在历次迭代中遇到的疑难问题、底层成因及最终工程解决方案。

> **版本同步强制规范**：每次完成迭代任务后，必须同步更新产品版本号（当前版本：**`0.1.2`**），涉及位置包括：
> 1. `src/index.html` -> `#settingsVersionText`（仅显示纯数字版本号如 `0.1.2`）；
> 2. `src-tauri/tauri.conf.json` -> `"version"`；
> 3. `src-tauri/Cargo.toml` -> `[package] version`；
> 4. `package.json` -> `"version"`；
> 5. `TECHNICAL_SPEC.md` 标题与正文版本记录。

---

## 一、产品调性与核心定位

1. **10-Foot 客厅大屏沉浸体验（tvOS × PSP XMB 融合美学）**
   - 产品定位于 macOS 全屏客厅媒体中心，融合 **Apple TV (tvOS)** 的毛玻璃层级、呼吸感字重与 **Sony PlayStation / PSP XMB（交叉媒体栏）** 的丝带流光背景及十字轴焦点导航体验。
   - **背景即正式播放舞台**：任何触发了后台视频流预览的画面均视为正在进行的正式播放。只要用户未聚焦到另一个可播放媒体卡片或主动长按停止，焦点在导航栏、丸形菜单或设置项之间移动时**绝不打断当前背景播放**。
   - **全局彻底隐藏鼠标指针（Round 18 新增）**：在应用程序窗口内部（含主页、全屏设置/密码/电源页及全屏播放画面），鼠标指针始终彻底不可见（全局 `cursor: none !important`）；仅当用户将鼠标指针移出 App 窗口边界时，系统鼠标指针才恢复可见。
2. **全键盘/遥控器优先，二维十字轴精准方向键导航**
   - 所有界面元素均可通过方向键（`ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight`）、确认键（`Enter`）、返回键（`Escape` / `Backspace`）及全局功能键（`F16` / `F17` / `F18` / `F19`）完成闭环导航。
   - **多行丸形菜单（`#seasonBar`）自动行检测与上下键跨行直达（Round 18 新增）**：
     - 当丸形菜单项目较多自动换行（`flex-wrap: wrap`）形成多行时，`findSeasonPillInAdjacentRow()` 自动按各按钮的视觉纵坐标（`rect.top`）聚类识别多行结构：
       - 按 `ArrowDown`：若当前丸形按钮下方仍有下一行丸形按钮，则直接移动到下一行水平中心（`centerX`）最接近的丸形按钮，无需一直按右键遍历完第一行；仅当已处于最后一行丸形按钮时，按 `ArrowDown` 才进入下方卡片轨道。
       - 按 `ArrowUp`：若当前丸形按钮上方仍有上一行丸形按钮，则直接移动到上一行水平中心最接近的丸形按钮；仅当已处于第一行丸形按钮时，按 `ArrowUp` 才折叠二级丸形菜单或返回左侧边栏。
   - **设置页与弹窗严格的“上下键跨行、左右键同行”二维导航规范（Round 18 新增）**：
     - 在设置页（`#settingsModal`）与密码页（`#passwordModal`）中，**`ArrowUp` / `ArrowDown` 只能用于在不同行之间上下移动，绝不横向切换同行元素；`ArrowLeft` / `ArrowRight` 只能用于在同一行内的并排控件（如 `[登录 抖音, 登录 bilibili]`、`[城市, 区, 街道]`、`[取消, 保存并扫描]`）或分段控制器（`.seg-control`）选项之间左右移动，绝不跨行跳转**。
   - **上键（`ArrowUp`）与 `ESC` 键等价返回规范（Round 17 新增）**：在主界面中，所有通过按 `ArrowUp` 实现返回层级的操作，必须同步支持按 `Escape` 键实现返回。
   - **全局功能快捷键规范（Round 17 & Round 18 演进）**：
     - **`F19`（全局电源键）**：在任何情况下直接唤出全屏电源选项页（`#powerModal`）覆盖当前画面。
     - **`F18`（主页与正式播放页双向切换键，Round 18 升级）**：
       - 若当前**未进入过正式播放页**（仅在主页浏览或仅触发了背景预览），按 `F18` 只能用于回到主页面并将焦点放置在左侧边栏 **“正在观看 / 现在观看”**（`#navNowPlayingItem`）导航项上；
       - 若当前**位于正式播放页**，按 `F18` 可回到主页面并将焦点放置在 `#navNowPlayingItem`；此后只要未切换其他视频流，在主页面 `#navNowPlayingItem` 再次按 `F18` 可直接返回正式播放页（`enterNowPlayingFullscreen()`）。
     - **`F17`（全局长按替代键）**：当用户不便长按回车键时，单击一次 `F17` 即可立即触发当前焦点元素的长按动作。
     - **`F16`（全局上下文菜单键 & 本地剧集选集呼出键，Round 18 新增）**：
       - 全局拦截并禁用 macOS WebKit 底层原生右键菜单（`contextmenu` 事件 `e.preventDefault(); e.stopPropagation();`，彻底屏蔽 `Reload`、`Play All Animations`、`Inspect Element`）；
       - 本地剧集在正式全屏播放期间，按 `F16` 键在画面底部唤出/收起选集列表遮罩（`#fsEpisodePickerHud`），支持左右键选集及通过左右两端 SVG `<` / `>` 箭头切换当前季。
3. **双空间隐私架构与屏幕数字键盘（常规空间 + 隐藏限制级空间）**
   - 默认界面与设置页对限制级功能实现**零视觉暴露**。
   - 在密码验证页长按密码输入框（或按 `F17`）可唤出/收起右侧大九键屏幕数字键盘（`#pwdNumpadPanel`，含 Apple 规范 `delete.left` 退格键）。

---

## 二、视觉与设计规范

### 1. 空间布局、侧边栏折叠方案与全局自适应缩放体系 (`autoScaleUI`)
- **基准视口与左侧边栏折叠方案（Round 18 新增）**：
  - 左侧边栏（`#sidebar`）固定定位于 `left: 20px; top: 20px; bottom: 20px;`，展开宽度 `205px`，折叠宽度 `68px`（`.sidebar.collapsed`）。
  - 在偏好设置页新增 **“左侧边栏折叠方案”**（`#btnToggleSidebarMode`，持久化于 `localStorage.fc_sidebar_mode`，默认 **`自动折叠`**）：
    - **`自动折叠` (`auto`)**：100% 保留现有逻辑，焦点在左侧边栏（`col === 0`）时展开，焦点移入右侧内容区（`col >= 1`）时自动折叠为胶囊态。
    - **`始终折叠` (`always`)**：左侧边栏始终保持 `.collapsed` 紧凑图标胶囊样式（主内容区始终 `.expanded`），同时用户依然可以 100% 正常移动焦点至左侧边栏各图标项进行导航、单击或长按操作。
- **全局缩放预设 (`UI_SCALE_PRESETS`)**：
  - 提供 `100%` ~ `200%` 六档缩放，严禁对 `body`、`#macVideo` 或 `#pspWaveCanvas` 应用 `zoom`。

### 2. 色彩系统、柔化底部黑色渐变遮罩与全屏遮罩色调联动
- **PSP XMB 动态丝带背景 (`PSP_BG_THEMES`)**：内置 6 套低饱和度深色主题及 `随机 random` 模式。
- **全屏播放进度条与选集列表柔化黑色透明渐变遮罩（Round 18 升级）**：
  - 全屏播放底部信息与进度条遮罩（`.iptv-fullscreen-hud`）以及本地剧集底部选集遮罩（`.fs-episode-picker-hud`）统一采用柔化透明度的黑色渐变遮罩：
    `background: linear-gradient(to top, rgba(0, 0, 0, 0.72) 0%, rgba(0, 0, 0, 0.42) 54%, rgba(0, 0, 0, 0) 100%);`
    最底部黑色部分增加适度透明度，避免生硬死黑，柔化大屏视觉体验。
  - **本地剧集底部选集列表（`#fsEpisodePickerHud`，Round 18 新增）**：横向排布集数丸形按钮（`.fs-ep-item-btn`），左右两端配备圆形毛玻璃季切换按钮（`#fsEpPrevSeasonBtn` / `#fsEpNextSeasonBtn`），**严格使用 SVG 矢量折线图标表示 `<` 与 `>`**（严禁直接使用纯文本字符 `<` 和 `>`）。

### 3. 字体规范与气象信息字体/字号规范 (Round 18 更新)
- **系统主字体栈**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Helvetica Neue", sans-serif`。
- **抖音美好体 (`DouyinMeihaoti` / `.douyin-font`)**：单独出现时不加空格，嵌入其他文字中时通过 `formatDouyinBrandTextHtml()` 在两侧自动加空格。
- **气象速报字体与字号规范 (`#jpWeatherTelop`，Round 18 更新)**：
  - **取消圆体，恢复之前的标准系统无衬线字体栈**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Hiragino Sans GB", "Helvetica Neue", sans-serif`。
  - **轻微调大字号**：主行字号由 `30px` 微调至 `33px`，次行发布机构字号由 `25px` 微调至 `27px`，并在 `autoScaleUI()` 中将 `--telop-scale` 基准微调至 `0.77 ~ 1.10`，清晰醒目且不夸张。

---

## 三、用户交互内容的文字措辞规范

1. **严禁出现按键括号提示或机械操作说明**
2. **设置页文案与层级命名规范**
   - 页面主标题：**`偏好设置`**，副标题：**`影视库资源，流媒体账户，显示和个性化设置。`**
   - 分区小标题：**`媒体与账户设置`**、**`个性化设置`**、**`限制级内容设置`**（默认隐藏）
   - 设置项名称：
     - `数据同步`、`影视库资源位置`、`EPG 电子节目单来源`、`流媒体账号连接`（`登录 抖音`、`登录 bilibili`）
     - `气象信息定位`（`自动定位` / `手动输入`）、`所在地位置`（`城市`、`区`、`街道`）
     - `影片评分来源`、`背景配色方案`、`用户界面显示比例`、**`左侧边栏折叠方案`（`自动折叠` / `始终折叠`）**、`预览时开启声音`、`音量均衡`
     - `限制级媒体资源位置`、`限制级内容密码`
   - 页面最底部版本号：**只显示纯版本号数字**（当前为 `0.1.2`）。

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

### 9. 正在观看/现在观看持久化记忆（上限 20 条）、卡片底部贴边白线进度条与“预览不入库、正式播放才入库”机制（Round 18 重点复盘）
- **核心设计规范 (`src/index.html`)**：
  1. **未进入正式播放的预览内容绝不计入历史记录 (`startPreviewStream` vs `executeItemAction`)**：
     - 焦点停留在卡片上触发的背景预览（`startPreviewStream`）仅调用 `syncNowWatchingRowData(false)`，使当前正在预览的内容临时作为“正在观看”页的第一张卡片展示；一旦用户预览其他内容或停止预览，该未进入正式播放的临时记录自动消失；
     - 只有当用户按回车/点击真正进入**正式全屏播放页面**（`isFullScreen = true`）时，才调用 `recordWatchHistoryItem(activePreviewMeta)` 将记录写入 `watchHistoryList` 并持久化至 `localStorage.fc_watch_history_v1`。
  2. **跨会话记忆保存、最多 20 条上限与观看进度自动保存**：
     - 应用退出重开后从 `localStorage.fc_watch_history_v1` 自动恢复历史观看列表，严格限制最多保存 **20 条**记录（超出自动截断最旧记录）；
     - 播放期间通过 `timeupdate`、`seeked`、`pause`、`exitFullscreenToMainView` 与 `beforeunload` 自动计算并持久化 `progressSec`、`durationSec` 与 `progressPct`，并在“正在观看 / 现在观看”卡片（`rowId === 7`）底部渲染贴边白色进度线条（`.card-watch-progress-track` / `.card-watch-progress-fill`），再次从历史记录进入正式播放时自动从上次观看进度续播。

