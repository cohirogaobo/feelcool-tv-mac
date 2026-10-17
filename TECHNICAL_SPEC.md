# Feel Cool TV 技术规范与架构维护手册 (v0.1.0 持续演进版)

本文档面向参与 **Feel Cool TV (`cooltv-mac`)** 后续迭代的开发人员，系统梳理产品的核心调性、视觉与交互设计规范、界面文案措辞准则，并重点复盘前端（WebKit / HTML5 / CSS3 / HLS.js）与后端（Tauri v2 / Rust / FFmpeg / Rouille）在历次迭代中遇到的疑难问题、底层成因及最终工程解决方案。

---

## 一、产品调性与核心定位

1. **10-Foot 客厅大屏沉浸体验（tvOS × PSP XMB 融合美学）**
   - 产品定位于 macOS 全屏客厅媒体中心，融合 **Apple TV (tvOS)** 的毛玻璃层级、呼吸感字重与 **Sony PlayStation / PSP XMB（交叉媒体栏）** 的丝带流光背景及十字轴焦点导航体验。
   - **背景即正式播放舞台**：任何触发了后台视频流预览的画面均视为正在进行的正式播放。只要用户未聚焦到另一个可播放媒体卡片或主动长按停止，焦点在导航栏、丸形菜单或设置项之间移动时**绝不打断当前背景播放**。
2. **全键盘/遥控器优先，兼顾鼠标/触控板直觉操作**
   - 所有界面元素均可通过方向键（`ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight`）、确认键（`Enter`）与返回键（`Escape` / `Backspace`）完成闭环导航。
   - 同时支持完整的鼠标点击、长按手势（`550ms` 阈值）与进度条拖拽，鼠标静止 `2.2s` 后自动隐藏全局光标（`html.app-cursor-hidden`）。
3. **双空间隐私架构（常规空间 + 隐藏限制级空间）**
   - 默认界面与设置页对限制级功能实现**零视觉暴露**：
     - 主界面通过长按左上角用户头像（`#avatarHeader`，`550ms`）唤起全屏密码验证页，验证通过后动态解锁左侧边栏“限制级”入口；再次长按头像即刻隐藏并锁定。
     - 设置页默认隐藏“限制级内容设置”分组，仅在设置页内长按底部“保存并扫描”按钮（`#btnSaveSettings`，`550ms`）时动态展开。

---

## 二、视觉与设计规范

### 1. 空间布局与全局自适应缩放体系 (`autoScaleUI`)
- **基准视口与双栏结构**：
  - 基准分辨率为 `1920 × 1080`，左侧边栏（`#sidebar`）固定定位于 `left: 20px; top: 20px; bottom: 20px;`，展开宽度 `205px`；当焦点进入右侧内容区（`col >= 1`）时，边栏收缩为 `68px` 胶囊态（`.sidebar.collapsed`），主内容区（`#mainContent`）由 `transform: translateX(137px)` 平滑归位至 `translateX(0)`。
- **五档全局缩放预设 (`UI_SCALE_PRESETS`)**：
  - 提供 `较小 (1.15)`、`标准 (1.28)`、`较大 (1.40，默认)`、`特大 (1.55)`、`极大 (1.75)` 五档缩放。
  - **缩放隔离原则**：仅对 `#sidebar`、`#mainContent` 及全屏遮罩页内容容器（`#settingsPageContent`、`#passwordPageContent`、`#powerPageContent`）应用 CSS `zoom`，**严禁对 `body`、`#macVideo` 或 `#pspWaveCanvas` 应用 `zoom`**，确保底层视频与 Canvas 始终严格保持 `100vw × 100vh` 满屏渲染。

### 2. 色彩系统、动态流光背景与全屏遮罩色调联动
- **PSP XMB 动态丝带背景 (`PSP_BG_THEMES`)**：
  - 内置 6 套低饱和度深色主题（`深海幽蓝 ocean`、`暮光星云 twilight`、`竹韵青灰 jade`、`暖珀流沙 amber`、`曜石银灰 graphite`、`绯樱夜雾 sakura`）以及默认开启的 `随机 random` 模式（每次应用启动时随机抽取一套主题存入 `sessionRandomTheme`）。
  - 仅在非全屏且无活跃背景视频播放时运行 Canvas 渲染循环，避免后台空耗 GPU。
- **全屏遮罩页面色调微调联动 (`syncOverlayThemeTint`)**：
  - 设置页（`#settingsModal`）、密码验证页（`#passwordModal`）与电源选项页（`#powerModal`）统一采用 `.fs-overlay-mask` 全屏毛玻璃遮罩设计（`backdrop-filter: blur(42px) saturate(135%)`）。
  - 遮罩背景通过 CSS 变量 `--overlay-bg-inner`（透明度 `0.76`）与 `--overlay-bg-outer`（透明度 `0.88`）读取当前激活配色的 `overlayInner` / `overlayOuter` 属性，在保持深邃暗调的基础上叠加 `±4 ~ ±8` RGB 通道的微妙色相偏移，做到既贴合当前主题色调又不显突兀。
- **设置页与密码/电源页对齐及弹性布局隔离规范**：
  - 设置页（`.fs-settings-page`）采用大屏居左排版（`margin: auto auto auto 0; text-align: left;`），左侧为固定宽度标题（`.setting-row-label`），右侧为水平弹性容器（`.setting-row-right`），单行多选统一采用横向分段长条形控制器（`.seg-control`）。
  - 密码页（`.fs-password-page`）与电源页保持居中纵向聚焦排版（`margin: auto; text-align: center; flex-direction: column;`）。
  - **严禁在全局 `.btn-full` 上直接声明 `flex: 1`**：因为 `flex: 1` 等价于 `flex: 1 1 0%`，在电源页纵向 Flex 容器（`flex-direction: column`）中会将按钮主轴基准高度（`flex-basis`）强制归零，从而覆盖 `height: 58px` 导致电源页 3 个按钮高度严重塌陷。必须将横向等分样式严格限定在 `.setting-row-right .btn-full, .setting-row-right .form-input { flex: 1; min-width: 0; }`。

### 3. 字体规范与抖音品牌字体内嵌空格规范 (`formatDouyinBrandTextHtml`)
- **系统主字体栈**：`-apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Helvetica Neue", sans-serif`。
- **抖音美好体 (`DouyinMeihaoti` / `.douyin-font`)**：
  - 字体资源位于 `src/assets/DouyinMeihaoti.otf`（根目录严禁遗留临时字体文件）。
  - **单独出现 vs. 嵌入文字中的空格规范（Round 16 强制规范）**：
    1. **单独出现时不加空格**：当 `抖音` 二字或抖音产品/分类字样（`抖音精选`、`抖音推荐`、`抖音知识`、`抖音二次元`、`抖音游戏`、`抖音美食`、`抖音体育`、`抖音时尚`、`抖音音乐`）作为独立文本出现时（例如英雄区主标题 `抖音`、丸形菜单按钮 `抖音精选`、卡片独立副标题 `抖音`），直接包裹 `<span class="douyin-font">...</span>`，**两侧严禁添加多余空格**。
    2. **插入其他文字中时两侧必须加空格**：当 `抖音` 二字或抖音产品字样嵌入在其他文字中间或一侧与非空白字符相邻时，必须通过 `formatDouyinBrandTextHtml()` 自动在其与相邻非空白字符之间插入半角空格（例如：`正在刷新 抖音 推荐视频流`、`正在刷新 抖音精选 视频流...`、`正在加载 抖音推荐 视频流...`、`登录 抖音`、`@李子柒· 抖音`）。
- **气象速报专用圆体 (`#jpWeatherTelop`)**：
  - 采用 `"STYuanti-SC-Bold", "Yuanti SC", "YouYuan", "Hiragino Maru Gothic ProN", sans-serif`，还原日式电视气象速报字幕质感。

### 4. 核心图标与动效规范
- **侧边栏“现在观看”与“很酷 TV+”图标**：
  - “很酷 TV+”固定使用带外圈的圆形播放 SVG；
  - “现在观看”在未播放或暂停时显示纯右向三角形播放符号（`<path d="M8 5v14l11-7z"/>`），在后台或全屏活跃播放时切换为三根动态跳动的音频波形条（`.wave-bars-svg`），且“现在观看”第一张正在播放卡片右上角同步展示动态波形角标。
- **流媒体丸形菜单两级架构**：
  - 一级菜单展示：`[刷新按钮] [抖音精选 ▸] [bilibili ▸]`；
  - 点击或按回车展开对应平台的二级分类菜单（抖音二级菜单全部带 `抖音` 前缀并使用抖音美好体；bilibili 二级菜单全部带 `bilibili` 前缀，如 `bilibili热门`、`bilibili推荐`、`bilibili动画`、`bilibili番剧`、`bilibili影视`、`bilibili游戏`、`bilibili知识`、`bilibili科技`、`bilibili音乐`、`bilibili美食`、`bilibili舞蹈`）；
  - 在二级菜单按 `ArrowUp` 丝滑折叠回一级菜单并恢复对应平台入口焦点。

---

## 三、用户交互内容的文字措辞规范

本项目对 UI 文案遵循 **“极简、克制、去机械感、原生 Apple TV 语感”** 的措辞准则，后续迭代新增或修改文案时必须严格遵守：

1. **严禁出现按键括号提示或机械操作说明**
   - 错误示例：`取消 (Esc)`、`使用方向键选择操作，按回车键确认以关闭或重启 Feel Cool TV。`、`关闭并退出 Feel Cool TV`
   - 正确规范：
     - 电源页标题：**`电源选项`**，副标题：**`选取选项以关闭或重新启动应用程序。`**
     - 电源页按钮：**`关闭并退出`**、**`重新启动`**、**`取消`**
     - 密码页标题：**`访问限制`**，副标题：**`输入密码以解锁限制级媒体内容。`**
2. **设置页文案与层级命名规范**
   - 页面主标题：**`偏好设置`**，副标题：**`影视库资源，流媒体账户，显示和个性化设置。`**
   - 分区小标题：**`媒体与账户设置`**、**`个性化设置`**、**`限制级内容设置`**（默认隐藏）
   - 设置项名称（精炼无赘字）：
     - `数据同步`：**时间戳严禁显示在文本框内**（`.sync-time-box` 必须保持 `background: transparent; border: none; text-align: right;`），直接靠右无边框显示在圆形刷新按钮（`#btnRefreshAllData`）左侧。
     - `影视库资源位置`
     - `EPG 电子节目单来源`
     - `流媒体账号连接`（右侧并排按钮：**`登录 抖音`**、**`登录 bilibili`**，注意“登录”与品牌名之间必须留有空格）
     - `气象信息定位`（横向分段控制器：`自动定位` / `手动输入`）
     - `所在地位置`（右侧并排开放三个文本输入框，占位提示分别为 **`城市`** `#inputLocCity`、**`区`** `#inputLocDistrict`、**`街道`** `#inputLocStreet`；用户在任一输入框输入内容时自动切换为 `手动输入` 模式）
     - `影片评分来源`（`豆瓣` / `TMDB`）
     - `配色`（`随机` / `深海幽蓝` / `暮光星云` / `竹韵青灰` / `暖珀流沙` / `曜石银灰` / `绯樱夜雾`）
     - `显示大小`（`较小` / `标准` / `较大` / `特大` / `极大`）
     - `预览声效`（`开启` / `静音`）
     - `音量均衡`（`开启` / `关闭`）
     - `限制级资源位置`、`限制级访问密码`
   - 底部主操作按钮：**`取消`**、**`保存并扫描`**
   - 页面最底部版本号：**只显示纯版本号数字**（如 `0.1.0`），严禁拼接产品名称或 `v` 前缀。
3. **卡片副标题与来源标识规范**
   - “现在观看 / 正在观看”板块（`rowId === 7`）卡片副标题末尾统一以中点 `·` 标注内容来源，格式为：`{原副标题}·{来源名称}`（例如：`@李子柒· 抖音`、`@何同学·bilibili`、`新闻联播·有线电视`、`豆瓣 9.2分·很酷 TV+`）。
4. **气象速报播报格式规范**
   - 第一行左侧：`气象速报 {天气现象} {温度}°C`（或特殊天气提醒如 `降水预报 15分钟后小雨`、`空气质量提醒 AQI 115`）；
   - 第一行右侧：**仅显示当前所在及邻近的 3 个街道名称**，以空格分隔（例如：`梅山街道 赛虹桥街道 铁心桥街道`）；
   - 第二行右侧：显示市级气象台署名（例如：`南京市气象台`）。

---

## 四、前端与后端代码注意事项（历史疑难问题与最终解决方案）

本节汇总了项目开发过程中曾导致严重视觉异常、卡顿或功能失效的代码细节及其根因与最终方案，后续开发人员在修改相关模块时**切勿回退或破坏以下机制**：

### 1. WebKit `AVSampleBufferDisplayLayer` 视频切流与几何过渡白屏/闪烁问题
- **历史异常表现**：
  - 在 macOS WKWebView 中，当 `<video>` 元素切换 HLS 源、或从全屏状态（`object-fit: contain`）退回背景预览状态（`object-fit: cover`）时，画面会出现瞬间白屏、黑场闪烁或上一部视频残留帧撕裂。
- **底层原因**：
  - 若在切流时设置 `display: none`、`visibility: hidden` 或 `opacity: 0`，macOS WebKit 会直接挂起或销毁底层的 `AVSampleBufferDisplayLayer` 硬件解码合成层；而当 `object-fit` 发生突变时，WebKit 合成器在缓冲帧未就绪前会短暂暴露底色。
- **最终解决方案 (`src/index.html` -> `hideVideoLayerInstant` / `loadAndPlayVideo`)**：
  1. **严禁使用 `opacity: 0` 或 `visibility: hidden` 隐藏视频层**：隐藏视频时必须设置 `macVideo.style.opacity = '0.002'` 且保持 `visibility: visible`。`0.002` 在人眼视觉上完全不可见，但能欺骗 WebKit 保持硬件解码管线与 GPU 合成层持续活跃。
  2. **几何稳定延迟显影（380ms Settle Window）**：视频触发 `playing` 且 `readyState >= 2`、`videoWidth > 0` 后，额外等待 `380ms`（待侧边栏/内容区 CSS 几何过渡完全静止），再经过双层 `requestAnimationFrame` 将 `opacity` 平滑过渡至 `1`。
  3. **同源全屏切换零重连**：当背景预览已就绪且进入全屏播放同一流地址时，直接复用当前 `hlsInstance` 与 `<video>` 缓冲管线，仅切换音频静音状态与 `object-fit`，不重启 FFmpeg。

### 2. 全屏进度条快进/快退与拖拽后的“回弹 (Rubber-banding)”及超出缓冲区跳转失效
- **历史异常表现**：
  - 用户连续按左右方向键或拖拽进度条松手后，进度条滑块会先跳回旧播放位置闪烁一下再跳向目标位置；且当快进目标超出当前 HLS 已生成的 `.ts` 分片范围时，直接修改 `macVideo.currentTime` 会被浏览器强制钳制在已缓冲末尾。
- **底层原因**：
  - HLS.js 与 FFmpeg 转码均为异步过程，松手瞬间 `macVideo.currentTime` 尚未更新，`timeupdate` 事件读回旧时间戳导致 UI 回弹；而未转码到的未来时间点在本地 `.m3u8` 中尚不存在。
- **最终解决方案 (`src/index.html` -> `commitSeekTo` & `src-tauri/src/lib.rs` -> `start_ffmpeg`)**：
  1. **时间戳锁定机制 (`lockedSeekTargetSec` + `lockedSeekExpireTs`)**：发起跳转时立即将目标秒数写入 `lockedSeekTargetSec` 并锁定最长 `6500ms`；在此期间 `getEffectiveCurrentTime()` 强制返回锁定值，直至 `seeked` / `timeupdate` 检测到真实播放进度已进入目标时间 `±2.2s` 范围内才解除锁定。
  2. **缓冲区内外双模跳转**：通过 `isLocalTimeBuffered(macVideo, localTarget)` 检测目标秒数是否已缓冲。若在缓冲区内，直接设置 `macVideo.currentTime` 实现零延迟跳转；若超出缓冲区，则调用后端 `start_ffmpeg(url, seekSec)` 通过 FFmpeg `-ss` 参数从目标时间点重新切片，同时传入 `keepLayerVisibleDuringSeek = true` 保持当前视频帧冻结显示直至新分片无缝衔接，彻底消除跳转黑屏。

### 3. 标题单行跑马灯 (`smoothMarquee`) 左侧遮罩遮字与循环结束闪烁问题
- **历史异常表现**：
  - 跑马灯添加左侧渐变透明遮罩后，静止阶段（`0% ~ 15%`）标题第一个字被左侧遮罩半透明削弱；且每当滚动周期结束（`100% -> 0%`）或后台轮询刷新时，标题会明显闪烁一下。
- **底层原因**：
  - 遮罩从容器 `0px` 开始渐变，直接覆盖了位于 `0px` 的首字符；
  - `.marquee-track` 使用 `display: inline-block` 时，两个子 `<span>` 在 WebKit 亚像素排版下总宽度的一半（`-50%`）与第二个 `<span>` 起点存在亚像素偏差，且父级 `-webkit-mask-image` 在子层循环跳变或 `innerHTML` 被重复赋值重建 DOM 时会触发 CoreAnimation 纹理重分配闪烁。
- **最终解决方案 (`src/index.html`)**：
  1. **负外边距左移遮罩区**：为 `.single-line-marquee` 设置 `margin-left: -26px; padding-left: 26px; -webkit-mask-image: linear-gradient(to right, transparent 0px, black 26px, black 93%, transparent 100%);`，使 `0px ~ 26px` 的透明过渡带严格位于静止首字左侧空白处，文字向左滚出时才进入渐隐区。
  2. **等宽 Flex 双轨 + GPU 合成层锁定 + DOM Diff 防重建**：
     - `.marquee-track` 采用 `display: inline-flex; width: max-content; transform: translate3d(0,0,0); backface-visibility: hidden;`，子项 `.marquee-track > span` 设置 `flex: 0 0 auto; box-sizing: border-box; padding-right: 72px;`，确保 `-50%` 位移在像素级完全重合；
     - 在 `updateHeroUI` 与 `showIptvFullscreenHud` 中通过 `data-marquee-key` 比对当前跑马灯文本，文本未变时**严禁重写 `innerHTML`**，防止轮询定时器打断正在运行的 CSS 动画。

### 4. 侧边栏收起/展开时图标跳动与导航项选中态左右白边被裁切
- **历史异常表现**：
  - 焦点在左侧边栏与右侧卡片间切换时，边栏 SVG 图标发生水平位移跳动；导航项获得白色选中背景（`scale(1.03)`）时左右两侧圆角被硬生生截断。
- **底层原因**：
  - 边栏收起时若改变容器 `left` 或将 `.nav-item` 改为 `justify-content: center`，会导致图标基准坐标改变；而 `.nav-list` 设置了 `overflow: hidden` 会裁切放大后的按钮边缘。
- **最终解决方案 (`src/index.html`)**：
  - `.sidebar` 固定 `left: 20px`，收起仅将 `width` 从 `205px` 过渡至 `68px`；所有 `.nav-item` 始终保持 `padding: 0 12px; justify-content: flex-start;`，文字 `.item-text` 通过 `max-width: 130px -> 0` 与 `opacity: 1 -> 0` 原地消隐，图标物理坐标分毫不差。
  - `.nav-list` 与 `.settings-section` 强制 `overflow: visible`。

### 5. 气象速报彩色下划线遮字、VPN/TUN 代理误定位“阿姆斯特丹”与自动/手动双模定位机制
- **历史异常表现**：
  - 气象速报标题的彩色半透明下划线若使用前景叠加会遮挡圆体汉字底部笔画；
  - 当系统开启全局/TUN 模式代理（如荷兰阿姆斯特丹节点）时，即便 `curl --noproxy "*"` 绕过了 HTTP 环境变量代理，虚拟网卡层仍会将流量路由至海外出口，导致 IP 定位接口返回 `阿姆斯特丹 (Amsterdam, ~52.37°N, 4.89°E)`，此前后端仅校验 `lat.abs() > 1.0` 即采信该坐标。
- **最终解决方案 (`src/index.html` & `src-tauri/src/lib.rs` -> `fetch_weather_broadcast`)**：
  1. **下划线置后**：`#telopMainText` 设置 `position: relative; z-index: 1; isolation: isolate;`，彩色下划线通过 `::after` 伪元素（`z-index: -1; bottom: 2px; height: 11px;`）置于文字涂层正后方。
  2. **自动定位与手动输入（城市、区、街道）双模架构**：
     - 设置页提供 `气象信息定位`（`自动定位 auto` / `手动输入 manual`）及 `所在地位置` 三个独立文本输入框（`城市`、`区`、`街道`）。
     - 当处于 **手动输入 (`manual`)** 模式时，后端直接使用用户填写的城市、区、街道，结合内置城市坐标表与街道邻接库生成 3 个邻近街道及对应市级气象台署名。
     - 当处于 **自动定位 (`auto`)** 模式时，前端传入 WebKit CoreLocation 坐标并由后端依次探测美团、哔哩哔哩、腾讯与 WeatherCN 接口；**所有候选坐标与地名必须通过三重境内校验**：
       - `is_china_coord(lat, lon)`：严格限定在中国境内经纬度包围盒（`16.0°N ~ 54.0°N, 73.0°E ~ 136.0°E`）；
       - `is_china_country(country)`：国家字段必须为空或明确为中国；
       - `!is_foreign_place_name(text)`：拦截 `阿姆斯特丹`、`Amsterdam`、`荷兰`、`Ashburn` 等海外节点名称。
       - 一旦检测到代理出口导致的境外定位，立即丢弃并回退至用户已保存的手动位置或境内默认基准位置（`南京 雨花台区 梅山街道`）。

### 6. 流媒体（抖音 & bilibili）首屏秒开、长视频分段流式加载與进入全屏默认播放态
- **历史异常表现（Round 16 重点复盘）**：
  1. **抖音推荐流冷启动无法加载或极慢，打开登录窗口再关闭后才恢复**：
     - **根因 A（跨启动去重黑名单误杀首屏）**：`ensure_douyin_scraper` 注入脚本此前从 `localStorage.getItem('__fc_boot_seen_v1__')` 读取最多 90 个历史 `aweme_id` 预填入 `window.__FC_SEEN__`。抖音首屏 SSR 与首包推荐流经常下发部分重叠 ID，导致冷启动首屏视频被静默过滤；而打开再关闭抖音登录窗口调用 `hide_douyin_login` 时清空了 `window.__FC_SEEN__` 并刷新页面，所以才恢复正常。
     - **根因 B（同步 `ffprobe` 阻塞卡片入队）**：后端收到 `__FC_ITEM__:` 时，此前在后台线程同步执行 `probe_douyin_candidates`（对多个候选地址跑 `ffprobe` 网络探测，耗时可达数秒至十余秒）后才将卡片推入 `douyin_feed`，导致前端长时间看不到任何卡片。
     - **根因 C（仅拦截 `fetch` 遗漏 XHR 与 SSR 首屏数据）**：抖音网页端部分推荐接口通过 `XMLHttpRequest` 或首屏 SSR（`window._SSR_DATA` / `#RENDER_DATA` / `self.__pace_f`）下发。
  2. **bilibili 推荐流长视频加载不出 & 进入正式播放页面时偶发处于暂停状态**：
     - **根因 A（整段大 MP4 头部探测阻塞）**：`x/player/playurl?platform=html5` 对长视频返回单个超大 MP4 文件（数百 MB），FFmpeg 默认会花费大量时间探测 `moov` 原子及流信息，且前端 HLS.js 配置了 `maxBufferLength: 60` 的大缓冲窗口。
     - **根因 B（预览流与全屏流并发竞态导致暂停）**：用户在卡片上停留满 300ms 刚触发 `startPreviewStream`（`start_ffmpeg` 尚在飞行中，`isVideoReady` 仍为 `false`）时立即按回车进入全屏，`executeItemAction` 会再次发起第二个 `start_ffmpeg` 杀死前一个进程，且在 HLS 首个分片缓冲完成前 `<video>` 处于 `paused` 状态，导致进入全屏后停留在暂停态。
- **最终解决方案 (`src-tauri/src/lib.rs` & `src/index.html`)**：
  1. **抖音首屏零阻塞入队 + 异步编码升级 + 全通道拦截**：
     - 移除 `__fc_boot_seen_v1__` 跨启动黑名单过滤，仅在单次页面生命周期内去重；
     - 同时 Hook `window.fetch`、`XMLHttpRequest.prototype.open/send` 以及首屏 SSR 数据（`window._SSR_DATA`、`#RENDER_DATA`、`self.__pace_f`）；当 `get_douyin_feed` 发现当前分类队列为空时，自动在隐藏 WebView 中触发 `window.__FC_LOAD_MORE__()`；
     - 收到 `__FC_ITEM__:` 时**立即将卡片推入 `douyin_feed`**（前端毫秒级渲染卡片），同时在后台异步运行 `probe_douyin_candidates`，探测完成后原地将 `play_url` 升级为 `h264:` 标记。
  2. **bilibili 长视频分段流式加载 + FFmpeg 低延迟流式切片**：
     - 在 `resolve_bilibili_stream_candidates` 中，针对时长 `> 300s` 的长视频优先请求 `platform=pc&fnval=1&qn=64` 分段流接口（返回较小分片流，无须下载超大单文件 `moov` 表）；
     - 在 `build_ffmpeg_args` 中为流媒体统一注入 `-seekable 1 -multiple_requests 1 -analyzeduration 1200000 -probesize 1500000 -fflags +nobuffer+genpts`，使 FFmpeg 在 `1.2s` 内即刻产出首个 `1s` `.ts` 分片；
     - 前端 `loadAndPlayVideo` 将 HLS.js 调整为轻量流式滑窗缓冲（`backBufferLength: 15, maxBufferLength: 20, maxMaxBufferLength: 40, startFragPrefetch: true`）。
  3. **进入全屏强制自动播放保障 (`userPausedInFullscreen` + `inFlightFfmpegReq` 共享)**：
     - `startPreviewStream` 与 `executeItemAction` 共享 `inFlightFfmpegReq` 飞行请求：若预览已发起同一 URL 的 `start_ffmpeg`，进入全屏时直接复用该请求并切换音频与全屏状态，绝不重复杀死重启 FFmpeg；
     - 引入 `userPausedInFullscreen` 显式状态位：进入全屏或上下切台时强制重置 `userPausedInFullscreen = false`，并在 `Hls.Events.MANIFEST_PARSED`、`Hls.Events.FRAG_BUFFERED` 与 `macVideo.oncanplay` 事件中多重兜底——只要 `!userPausedInFullscreen && macVideo.paused`，一旦分片就绪立即调用 `startVideoPlaybackWithFallback` 启动播放，同时底部 HUD 图标在未主动暂停前始终显示“播放中（双竖线暂停图标）”。

### 7. FFmpeg 进程生命周期与本地大目录空闲期抽帧调度
- **注意事项**：
  - 每次调用 `start_ffmpeg` 时必须递增 `ffmpeg_generation: AtomicU64` 并在 `ffmpeg_op_lock` 互斥锁内先 `kill() + wait()` 旧进程；应用启动、退出（`quit_app`）与重启（`restart_app`）时均调用 `cleanup_orphan_hls_ffmpeg()` 清理 `/tmp/cooltv_hls/seg_` 孤儿进程。
  - 本地视频与限制级目录缩略图截取（`extract_video_thumb`）必须受前端 `uiMotionUntil` 空闲检测与后端 `thumb_lock` 双重节流控制，仅在焦点移动动画完全静止后以单并发串行抽帧，严禁在焦点滑动期间并发启动多个 FFmpeg 进程抢占 CPU/GPU。

### 8. 电源选项“重新启动”白屏问题复盘与进程内热重启方案
- **历史异常表现**：
  - 在电源选项页面点击“重新启动”（`#btnPowerRestart`）后，应用窗口变成一片白屏且无法恢复。
- **底层原因**：
  - 原 `restart_app` 调用了 `app.restart()`（进程级重生）。在 `tauri dev` 开发模式或 macOS 全屏独立空间（Fullscreen Space）下，新进程启动时旧进程尚未完全释放 `8765` 本地 HLS 端口与 WebKit GPU 进程锁，且原生 `NSWindow` 在销毁重建或尚未绘制首帧前会暴露出 `WKWebView` 默认的纯白底色。
- **最终解决方案 (`src-tauri/src/lib.rs` -> `restart_app` & `src-tauri/tauri.conf.json`)**：
  1. **原生窗口黑底锁死**：在 `tauri.conf.json` 的窗口配置中显式声明 `"backgroundColor": "#000000"`，从 macOS `NSWindow` / `WKWebView` 底层根除任何页面重载期间的白色闪屏。
  2. **进程内无感热重启 (`restart_app`)**：点击“重新启动”时，前端先将 `body` 平滑渐隐至纯黑（`#000`），后端 `restart_app` 在当前进程内原子化执行：递增 `ffmpeg_generation`、终止当前 FFmpeg 子进程、清理 `/tmp/cooltv_hls` 临时目录、清空 `douyin_feed` 与 `douyin_probing` 状态池、通知隐藏抓取器重新加载首屏，最后对主窗口执行 `window.location.reload()`，实现 100% 零白屏、零端口冲突的极速重启。
