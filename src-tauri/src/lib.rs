use std::process::{Command, Child, Stdio};
use std::sync::{Mutex, Arc};
use std::sync::atomic::{AtomicU64, Ordering};
use std::fs;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use tauri::{State, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, TitleBarStyle, WindowEvent};
use std::time::{SystemTime, UNIX_EPOCH, Duration};

type DouyinItem = (String, String, String, String);

struct AppState {
    ffmpeg_process: Arc<Mutex<Option<(u64, Child)>>>,
    ffmpeg_op_lock: Arc<Mutex<()>>,
    ffmpeg_generation: Arc<AtomicU64>,
    douyin_feed: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
    douyin_probing: Arc<Mutex<HashSet<String>>>,
    douyin_probe_lock: Arc<Mutex<()>>,
    douyin_category: Arc<Mutex<String>>,
    bili_cookie: Arc<Mutex<String>>,
    bili_logged_in_cats: Arc<Mutex<HashSet<String>>>,
    bili_page_map: Arc<Mutex<HashMap<String, u32>>>,
    thumb_lock: Arc<Mutex<()>>,
    minutecast_400_until: Arc<AtomicU64>,
    was_precipitating: Arc<Mutex<bool>>,
}

fn get_ffmpeg_bin() -> &'static str {
    if Path::new("/opt/homebrew/bin/ffmpeg").exists() {
        "/opt/homebrew/bin/ffmpeg"
    } else if Path::new("/usr/local/bin/ffmpeg").exists() {
        "/usr/local/bin/ffmpeg"
    } else {
        "ffmpeg"
    }
}

fn get_ffprobe_bin() -> &'static str {
    if Path::new("/opt/homebrew/bin/ffprobe").exists() {
        "/opt/homebrew/bin/ffprobe"
    } else if Path::new("/usr/local/bin/ffprobe").exists() {
        "/usr/local/bin/ffprobe"
    } else {
        "ffprobe"
    }
}

fn cleanup_orphan_hls_ffmpeg() {
    let _ = Command::new("/usr/bin/pkill")
        .args(&["-9", "-f", "/tmp/cooltv_hls/seg_"])
        .status();
}

#[tauri::command]
async fn start_drag(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.start_dragging();
    }
    Ok(())
}

fn apply_no_proxy_env(cmd: &mut Command) {
    cmd.env("no_proxy", "*")
        .env("NO_PROXY", "*")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY");
}

fn extract_stream_meta_tag<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("@@{}=", key);
    if let Some(idx) = raw.find(&pat) {
        let rest = &raw[idx + pat.len()..];
        return Some(rest.split("@@").next().unwrap_or(rest).trim());
    }
    None
}

fn strip_stream_meta_suffix(raw: &str) -> (&str, Option<f64>, Option<&str>) {
    let trimmed = raw.trim();
    let title_opt = extract_stream_meta_tag(trimmed, "title");
    let dur_opt = extract_stream_meta_tag(trimmed, "dur")
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|d| d.is_finite() && *d > 0.0);

    let mut cut_idx = trimmed.len();
    for tag in ["@@title=", "@@dur=", "@@aweme_id=", "@@sec_uid=", "@@mid="] {
        if let Some(i) = trimmed.find(tag) {
            if i < cut_idx {
                cut_idx = i;
            }
        }
    }
    (trimmed[..cut_idx].trim(), dur_opt, title_opt)
}

fn build_ffmpeg_args(
    actual_url: &str,
    is_local_file: bool,
    is_douyin: bool,
    is_bilibili: bool,
    use_sw_fallback: bool,
    skip_audio_filter: bool,
    seek_sec: Option<f64>,
    ts_pattern: &str,
    m3u8_path: &str,
) -> Vec<String> {
    let mut args = vec![
        "-nostdin".to_string(),
        "-v".to_string(),
        "error".to_string(),
    ];
    let v_codec = if use_sw_fallback { "libx264" } else { "h264_videotoolbox" };
    // 任务 1 (Round 5) & 任务 5 (Round 19) & 问题 1 (Round 25)：全链路音量均衡、峰值硬限幅与 aresample + asetpts=PTS-STARTPTS 音画零点时间戳严格对齐
    let audio_norm_std = "aresample=async=1:first_pts=0,asetpts=PTS-STARTPTS,acompressor=threshold=-18dB:ratio=3.5:attack=5:release=85:makeup=1,alimiter=limit=0.75:attack=5:release=50";
    let audio_norm_douyin = "aresample=async=1:first_pts=0,asetpts=PTS-STARTPTS,acompressor=threshold=-20dB:ratio=5:attack=4:release=75:makeup=1,alimiter=limit=0.55:attack=4:release=45";
    let audio_sync_only = "aresample=async=1:first_pts=0,asetpts=PTS-STARTPTS";
    let ss_val = match seek_sec {
        Some(s) if s >= 0.0 => format!("{:.2}", s),
        _ => "2".to_string(),
    };

    if is_local_file {
        // 任务 6 (Round 31)：针对 1918×802 等非 16 宏块对齐超宽屏及 23.98fps H.264 本地媒体：
        // 1) 统一等比缩放并居中填充至 1280×720（80×45 完整 16×16 宏块，SPS frame_cropping_flag=0，标准 16:9 画布），彻底消除非 16 对齐分辨率与非 16:9 纹理在 WebKit TiledCoreAnimationDrawingArea 下的宏块错位切块与瓦片冻结；
        // 2) 默认采用 libx264 ultrafast + zerolatency + sliced-threads=0:ref=1:bframes=0:force-cfr=1 单 Slice 整帧编码并剥离源流私有未注册 SEI (remove_types=6)，配合 -max_interleave_delta 0 与零点 PTS 对齐，彻底根除播放卡住（暂停继续后恢复）与切块异常。
        let local_v_codec = if use_sw_fallback { "h264_videotoolbox" } else { "libx264" };
        args.extend(vec![
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-err_detect".to_string(), "ignore_err".to_string(),
            "-analyzeduration".to_string(), "10M".to_string(),
            "-probesize".to_string(), "10M".to_string(),
            "-threads".to_string(), "4".to_string(),
            "-ss".to_string(), ss_val,
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-fps_mode".to_string(), "cfr".to_string(),
            "-r".to_string(), "30".to_string(),
            "-vf".to_string(), "setpts=PTS-STARTPTS,scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30,format=yuv420p".to_string(),
            "-c:v".to_string(), local_v_codec.to_string(),
            "-profile:v".to_string(), "main".to_string(),
            "-bf".to_string(), "0".to_string(),
        ]);
        if local_v_codec == "libx264" {
            args.extend(vec![
                "-preset".to_string(), "ultrafast".to_string(),
                "-tune".to_string(), "zerolatency".to_string(),
                "-level".to_string(), "4.0".to_string(),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-g".to_string(), "30".to_string(),
                "-keyint_min".to_string(), "30".to_string(),
                "-sc_threshold".to_string(), "0".to_string(),
                "-flags".to_string(), "+cgop".to_string(),
                "-x264-params".to_string(), "sliced-threads=0:ref=1:bframes=0:force-cfr=1".to_string(),
                "-bsf:v".to_string(), "filter_units=remove_types=6".to_string(),
                "-b:v".to_string(), "6000k".to_string(),
                "-maxrate".to_string(), "8500k".to_string(),
                "-bufsize".to_string(), "12000k".to_string(),
            ]);
        } else {
            args.extend(vec![
                "-realtime".to_string(), "1".to_string(),
                "-allow_sw".to_string(), "1".to_string(),
                "-g".to_string(), "30".to_string(),
                "-flags".to_string(), "+cgop".to_string(),
                "-bsf:v".to_string(), "filter_units=remove_types=6".to_string(),
                "-b:v".to_string(), "6000k".to_string(),
                "-maxrate".to_string(), "8500k".to_string(),
                "-bufsize".to_string(), "12000k".to_string(),
            ]);
        }
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_std.to_string()]);
        } else {
            args.extend(vec!["-af".to_string(), audio_sync_only.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "192k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-max_interleave_delta".to_string(), "0".to_string(),
            "-avoid_negative_ts".to_string(), "make_zero".to_string(),
            "-muxdelay".to_string(), "0".to_string(),
            "-muxpreload".to_string(), "0".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_playlist_type".to_string(), "event".to_string(),
            "-hls_flags".to_string(), "independent_segments+temp_file".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    } else if is_douyin || is_bilibili {
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        let header_lines = if is_bilibili {
            "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n"
        } else {
            "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n"
        };
        args.extend(vec![
            "-user_agent".to_string(), ua.to_string(),
            "-headers".to_string(), header_lines.to_string(),
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_at_eof".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-seekable".to_string(), "1".to_string(),
            "-thread_queue_size".to_string(), "1024".to_string(),
            "-analyzeduration".to_string(), "2000000".to_string(),
            "-probesize".to_string(), "2000000".to_string(),
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-err_detect".to_string(), "ignore_err".to_string(),
            "-threads".to_string(), "2".to_string(),
        ]);
        if let Some(s) = seek_sec {
            if s > 0.05 {
                args.extend(vec!["-ss".to_string(), format!("{:.2}", s)]);
            }
        }
        // 问题 1 (Round 25 重大故障根治)：
        // 1) 移除 -multiple_requests 1，防止非交错 MP4 发起 Range 请求时复用半关闭连接触发 CDN ECONNRESET 截断帧数据；
        // 2) 通过 setpts=PTS-STARTPTS + asetpts=PTS-STARTPTS + -muxdelay 0 -muxpreload 0 消除 MPEG-TS 默认 1.42s 初始偏移及音画起点差，杜绝 hls.js 在首分片触发 +0.1s 跳帧落入非关键帧 (P-frame) 造成参考帧丢失切块；
        // 3) 将任意横竖屏流媒体统一等比缩放到 1280x720 (80x45 个完整 16x16 宏块, frame_cropping_flag=0) 标准 16:9 画布内居中渲染 (pad=1280:720)，消除竖屏流在 WKWebView 负 z-index 层下 cover/contain 切换引发的 512x512 CoreAnimation 瓦片局部冻结；
        // 4) 采用 libx264 ultrafast + sliced-threads=0:ref=1:bframes=0:force-cfr=1 单 Slice 整帧极速编码（达 10x 实时速率）+ 剥离私有未注册 SEI (remove_types=6)。
        let stream_v_codec = if use_sw_fallback { "h264_videotoolbox" } else { "libx264" };
        args.extend(vec![
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-fps_mode".to_string(), "cfr".to_string(),
            "-r".to_string(), "30".to_string(),
            "-vf".to_string(), "setpts=PTS-STARTPTS,scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30,format=yuv420p".to_string(),
            "-c:v".to_string(), stream_v_codec.to_string(),
            "-profile:v".to_string(), "main".to_string(),
            "-bf".to_string(), "0".to_string(),
        ]);
        if stream_v_codec == "libx264" {
            args.extend(vec![
                "-preset".to_string(), "ultrafast".to_string(),
                "-tune".to_string(), "zerolatency".to_string(),
                "-level".to_string(), "4.0".to_string(),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-g".to_string(), "30".to_string(),
                "-keyint_min".to_string(), "30".to_string(),
                "-sc_threshold".to_string(), "0".to_string(),
                "-flags".to_string(), "+cgop".to_string(),
                "-x264-params".to_string(), "sliced-threads=0:ref=1:bframes=0:force-cfr=1".to_string(),
                "-bsf:v".to_string(), "filter_units=remove_types=6".to_string(),
                "-b:v".to_string(), "4500k".to_string(),
                "-maxrate".to_string(), "6500k".to_string(),
                "-bufsize".to_string(), "9000k".to_string(),
            ]);
        } else {
            args.extend(vec![
                "-realtime".to_string(), "1".to_string(),
                "-allow_sw".to_string(), "1".to_string(),
                "-g".to_string(), "30".to_string(),
                "-flags".to_string(), "+cgop".to_string(),
                "-bsf:v".to_string(), "filter_units=remove_types=6".to_string(),
                "-b:v".to_string(), "4500k".to_string(),
                "-maxrate".to_string(), "6500k".to_string(),
                "-bufsize".to_string(), "9000k".to_string(),
            ]);
        }
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_douyin.to_string()]);
        } else {
            args.extend(vec!["-af".to_string(), audio_sync_only.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "160k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-max_interleave_delta".to_string(), "0".to_string(),
            "-avoid_negative_ts".to_string(), "make_zero".to_string(),
            "-muxdelay".to_string(), "0".to_string(),
            "-muxpreload".to_string(), "0".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "0".to_string(),
            "-hls_playlist_type".to_string(), "event".to_string(),
            "-hls_flags".to_string(), "independent_segments+temp_file".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    } else {
        // 任务 3 (Round 17)：修复国内 IPTV 直播源黑屏（注入标准浏览器 User-Agent、移除导致 HLS 302/切片刷挂起的 -reconnect_streamed、支持 1080i 隔行扫描自动反交错 yadif 与可选音轨映射）
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        args.extend(vec![
            "-user_agent".to_string(), ua.to_string(),
            "-reconnect".to_string(), "1".to_string(),
            "-reconnect_delay_max".to_string(), "2".to_string(),
            "-rw_timeout".to_string(), "8000000".to_string(),
            "-analyzeduration".to_string(), "2200000".to_string(),
            "-probesize".to_string(), "2200000".to_string(),
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-i".to_string(), actual_url.to_string(),
            "-map".to_string(), "0:v:0".to_string(),
            "-map".to_string(), "0:a:0?".to_string(),
            "-fps_mode".to_string(), "cfr".to_string(),
            "-r".to_string(), "30".to_string(),
            "-vf".to_string(), "setpts=PTS-STARTPTS,yadif=0:-1:1,scale=trunc(iw/2)*2:trunc(ih/2)*2,fps=30,format=yuv420p".to_string(),
            "-c:v".to_string(), v_codec.to_string(),
            "-profile:v".to_string(), "main".to_string(),
            "-bf".to_string(), "0".to_string(),
        ]);
        if use_sw_fallback {
            args.extend(vec!["-preset".to_string(), "ultrafast".to_string()]);
        } else {
            args.extend(vec![
                "-realtime".to_string(), "1".to_string(),
                "-allow_sw".to_string(), "1".to_string(),
            ]);
        }
        args.extend(vec![
            "-b:v".to_string(), "6M".to_string(),
            "-g".to_string(), "30".to_string(),
        ]);
        if !skip_audio_filter {
            args.extend(vec!["-af".to_string(), audio_norm_std.to_string()]);
        } else {
            args.extend(vec!["-af".to_string(), audio_sync_only.to_string()]);
        }
        args.extend(vec![
            "-c:a".to_string(), "aac".to_string(),
            "-b:a".to_string(), "128k".to_string(),
            "-ar".to_string(), "44100".to_string(),
            "-ac".to_string(), "2".to_string(),
            "-avoid_negative_ts".to_string(), "make_zero".to_string(),
            "-muxdelay".to_string(), "0".to_string(),
            "-muxpreload".to_string(), "0".to_string(),
            "-f".to_string(), "hls".to_string(),
            "-hls_time".to_string(), "1".to_string(),
            "-hls_list_size".to_string(), "10".to_string(),
            "-hls_flags".to_string(), "delete_segments+independent_segments+temp_file".to_string(),
            "-hls_segment_filename".to_string(), ts_pattern.to_string(),
            m3u8_path.to_string()
        ]);
    }

    args
}

fn probe_douyin_candidates(raw_candidates: &str) -> Option<(String, String, Option<f64>)> {
    let (clean_candidates, hint_dur, _) = strip_stream_meta_suffix(raw_candidates);
    let ffprobe_bin = get_ffprobe_bin();
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let headers = "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n";

    let mut verified_urls: Vec<String> = Vec::new();
    let mut final_dur: Option<f64> = hint_dur;

    for cand in clean_candidates.split("@@") {
        let u = cand.trim().trim_start_matches("h264:");
        if !u.starts_with("https://") {
            continue;
        }
        let mut cmd = Command::new(ffprobe_bin);
        apply_no_proxy_env(&mut cmd);
        cmd.stdin(Stdio::null()).stderr(Stdio::null());
        if let Ok(out) = cmd
            .args(&[
                "-v", "error",
                "-user_agent", ua,
                "-headers", headers,
                "-rw_timeout", "3200000",
                "-analyzeduration", "500000",
                "-probesize", "500000",
                "-select_streams", "v:0",
                "-show_entries", "stream=codec_name,pix_fmt:format=duration",
                "-of", "default=noprint_wrappers=1",
                u,
            ])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut codec = String::new();
                let mut pix_fmt = String::new();
                for line in text.lines() {
                    let l = line.trim();
                    if let Some(c) = l.strip_prefix("codec_name=") {
                        codec = c.trim().to_lowercase();
                    } else if let Some(pf) = l.strip_prefix("pix_fmt=") {
                        pix_fmt = pf.trim().to_lowercase();
                    } else if let Some(d) = l.strip_prefix("duration=") {
                        if final_dur.is_none() {
                            if let Ok(val) = d.trim().parse::<f64>() {
                                if val.is_finite() && val > 0.0 {
                                    final_dur = Some(val);
                                }
                            }
                        }
                    }
                }
                // 问题 1 (Round 25)：严格只放行 8-bit yuv420p 标准 H.264 源，排除 ByteVC1/HEVC 与 10-bit HDR 流，并保留最多 2 个已验证备用 CDN 节点
                let is_8bit_420 = pix_fmt.is_empty() || pix_fmt == "yuv420p" || pix_fmt == "yuvj420p";
                if codec == "h264" && is_8bit_420 {
                    if !verified_urls.contains(&u.to_string()) {
                        verified_urls.push(u.to_string());
                    }
                    if verified_urls.len() >= 2 {
                        break;
                    }
                }
            }
        }
    }
    if !verified_urls.is_empty() {
        return Some((verified_urls.join("@@"), "h264".to_string(), final_dur));
    }
    None
}

fn resolve_iptv_stream_candidates(raw_input: &str) -> Vec<(String, bool)> {
    let (primary_url, _, title_opt) = strip_stream_meta_suffix(raw_input);
    let title = title_opt.unwrap_or("").trim();
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

    let is_overseas_channel = title.contains("CNA")
        || title.contains("亚洲新闻")
        || title.contains("NTV")
        || primary_url.contains("mediacorp")
        || primary_url.contains("cloudfront.net")
        || primary_url.contains("akamaized.net")
        || primary_url.contains("ntv.co.jp");

    let mut out: Vec<(String, bool)> = Vec::new();
    let mut push_unique = |u: String, direct_no_proxy: bool| {
        let s = u.trim().to_string();
        if !s.is_empty() && !out.iter().any(|(x, np)| x == &s && *np == direct_no_proxy) {
            out.push((s, direct_no_proxy));
        }
    };

    // 针对中国国内 IPTV 源：先用 curl --noproxy "*" 解析 HTTP 302 跳转后的真实地址（解决中移动 PLTV / 广电 302 相对路径丢失问题）
    if !is_overseas_channel && primary_url.starts_with("http") && !primary_url.contains("://[") {
        let mut cmd = Command::new("/usr/bin/curl");
        apply_no_proxy_env(&mut cmd);
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "-k",
            "--connect-timeout",
            "2",
            "--max-time",
            "3",
            "-A",
            ua,
            "-r",
            "0-1024",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}|||%{url_effective}",
            primary_url,
        ]);
        if let Ok(res) = cmd.output() {
            let txt = String::from_utf8_lossy(&res.stdout);
            let parts: Vec<&str> = txt.trim().split("|||").collect();
            let code = parts.first().and_then(|c| c.parse::<u16>().ok()).unwrap_or(0);
            let eff = parts.get(1).copied().unwrap_or("").trim();
            if (200..400).contains(&code) && eff.starts_with("http") {
                push_unique(eff.to_string(), true);
            }
        }
    }

    if !is_overseas_channel {
        push_unique(primary_url.to_string(), true);
        push_unique(primary_url.to_string(), false);
    } else {
        push_unique(primary_url.to_string(), false);
        push_unique(primary_url.to_string(), true);
    }

    out
}

fn resolve_bilibili_stream_candidates(raw_spec: &str, cookie: &str) -> (Vec<String>, Option<f64>) {
    let (clean_spec, suffix_dur, _) = strip_stream_meta_suffix(raw_spec);
    let clean = clean_spec
        .trim()
        .trim_start_matches("bilibili:")
        .trim_start_matches("douyin:")
        .trim_start_matches("bili:");
    let parts: Vec<&str> = clean.split("@@").collect();
    let bvid = parts.first().copied().unwrap_or("").trim();
    if bvid.is_empty() {
        return (Vec::new(), suffix_dur);
    }
    let mut cid: u64 = parts
        .get(1)
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let mut duration_opt: Option<f64> = parts
        .get(2)
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|d| *d > 0.0)
        .or(suffix_dur);

    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
    let curl_bili_json = |api_url: &str| -> Option<serde_json::Value> {
        let mut cmd = Command::new("/usr/bin/curl");
        apply_no_proxy_env(&mut cmd);
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--connect-timeout",
            "4",
            "--max-time",
            "6",
            "-H",
            &format!("User-Agent: {}", ua),
            "-H",
            "Referer: https://www.bilibili.com/",
        ]);
        if !cookie.trim().is_empty() {
            cmd.args(&["-H", &format!("Cookie: {}", cookie.trim())]);
        }
        cmd.arg(api_url);
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
    };

    if cid == 0 || duration_opt.is_none() {
        let view_url = format!("https://api.bilibili.com/x/web-interface/view?bvid={}", bvid);
        if let Some(vj) = curl_bili_json(&view_url) {
            if let Some(d) = vj.get("data") {
                if cid == 0 {
                    cid = d.get("cid").and_then(|v| v.as_u64()).unwrap_or(0);
                }
                if duration_opt.is_none() {
                    duration_opt = d.get("duration").and_then(|v| v.as_f64()).filter(|x| *x > 0.0);
                }
            }
        }
    }

    if cid == 0 {
        return (Vec::new(), duration_opt);
    }

    // 任务 6 (Round 16)：针对 bilibili 长视频（> 300s），优先请求分段式流式接口（fnval=1&platform=pc），避免单文件超大 moov 索引导致首包阻塞；同时保留 html5 高清源作为双保险
    let is_long_video = duration_opt.unwrap_or(0.0) > 300.0;
    let ep_html5 = format!(
        "https://api.bilibili.com/x/player/playurl?bvid={}&cid={}&qn=64&type=&otype=json&platform=html5&high_quality=1",
        bvid, cid
    );
    let ep_pc_stream = format!(
        "https://api.bilibili.com/x/player/playurl?bvid={}&cid={}&qn=64&fnval=1&platform=pc&otype=json",
        bvid, cid
    );
    let api_endpoints = if is_long_video {
        [ep_pc_stream, ep_html5]
    } else {
        [ep_html5, ep_pc_stream]
    };

    let mut urls: Vec<String> = Vec::new();
    for ep in &api_endpoints {
        if let Some(pj) = curl_bili_json(ep) {
            if let Some(d) = pj.get("data") {
                if duration_opt.is_none() {
                    if let Some(tl) = d.get("timelength").and_then(|v| v.as_f64()) {
                        if tl > 0.0 {
                            duration_opt = Some(tl / 1000.0);
                        }
                    }
                }
                if let Some(durl_arr) = d.get("durl").and_then(|v| v.as_array()) {
                    for item in durl_arr {
                        if let Some(u) = item.get("url").and_then(|v| v.as_str()) {
                            let s = u.trim().to_string();
                            if s.starts_with("http") && !urls.contains(&s) {
                                urls.push(s);
                            }
                        }
                        if let Some(b_arr) = item.get("backup_url").and_then(|v| v.as_array()) {
                            for b in b_arr {
                                if let Some(bu) = b.as_str() {
                                    let s = bu.trim().to_string();
                                    if s.starts_with("http") && !urls.contains(&s) {
                                        urls.push(s);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !urls.is_empty() {
            break;
        }
    }

    // 任务 1 (Round 25)：优先选择官方高带宽 upos-* 主干 CDN 节点，将不稳定的 P2P 边缘节点（mcdn.bilivideo.cn / szbdyd.com）降级至队尾，防止读取抖动导致中途断流卡死
    urls.sort_by_key(|u| {
        let lower = u.to_lowercase();
        if lower.contains("upos-") && !lower.contains("mcdn") && !lower.contains("szbdyd") {
            0
        } else if lower.contains("mcdn") || lower.contains("szbdyd") || lower.contains("p2p") {
            2
        } else {
            1
        }
    });

    (urls, duration_opt)
}

#[tauri::command]
async fn get_media_duration(url: String, state: State<'_, AppState>) -> Result<f64, String> {
    if let (_, Some(d), _) = strip_stream_meta_suffix(&url) {
        if d > 0.0 {
            return Ok(d);
        }
    }
    let bili_cookie = state.bili_cookie.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if url.starts_with("bilibili:") || url.starts_with("douyin:bili:") || url.starts_with("bili:") {
            let (clean_spec, _, _) = strip_stream_meta_suffix(&url);
            let clean = clean_spec
                .trim()
                .trim_start_matches("bilibili:")
                .trim_start_matches("douyin:")
                .trim_start_matches("bili:");
            let parts: Vec<&str> = clean.split("@@").collect();
            if let Some(inline_dur) = parts.get(2).and_then(|s| s.trim().parse::<f64>().ok()) {
                if inline_dur.is_finite() && inline_dur > 0.0 {
                    return Ok(inline_dur);
                }
            }
            let (cands, api_dur) = resolve_bilibili_stream_candidates(&url, &bili_cookie);
            if let Some(d) = api_dur {
                if d.is_finite() && d > 0.0 {
                    return Ok(d);
                }
            }
            if let Some(first_u) = cands.first() {
                let ffprobe_bin = get_ffprobe_bin();
                let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
                let headers = "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n";
                let mut cmd = Command::new(ffprobe_bin);
                apply_no_proxy_env(&mut cmd);
                if let Ok(out) = cmd
                    .args(&[
                        "-v", "error",
                        "-user_agent", ua,
                        "-headers", headers,
                        "-rw_timeout", "5000000",
                        "-show_entries", "format=duration",
                        "-of", "default=noprint_wrappers=1:nokey=1",
                        first_u,
                    ])
                    .output()
                {
                    if out.status.success() {
                        if let Ok(dur) = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>() {
                            if dur.is_finite() && dur > 0.0 {
                                return Ok(dur);
                            }
                        }
                    }
                }
            }
            return Err("无法获取 bilibili 视频时长".into());
        }

        let (clean_url, _, _) = strip_stream_meta_suffix(&url);
        let raw_url = if let Some(s) = clean_url.strip_prefix("douyin:h264:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else if let Some(s) = clean_url.strip_prefix("douyin:") {
            s.split("@@").next().unwrap_or(s).to_string()
        } else {
            clean_url.to_string()
        };
        let ffprobe_bin = get_ffprobe_bin();
        let mut cmd = Command::new(ffprobe_bin);
        apply_no_proxy_env(&mut cmd);
        cmd.args(&["-v", "error"]);
        if raw_url.starts_with("http") {
            let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
            let is_bili = raw_url.contains("bilivideo.") || raw_url.contains("bilibili.com") || raw_url.contains("hdslb.com") || raw_url.contains("szbdyd.com");
            let headers = if is_bili {
                "Referer: https://www.bilibili.com/\r\nOrigin: https://www.bilibili.com\r\n"
            } else {
                "Referer: https://www.douyin.com/\r\nOrigin: https://www.douyin.com\r\n"
            };
            cmd.args(&["-user_agent", ua, "-headers", headers, "-rw_timeout", "5000000"]);
        }
        cmd.args(&[
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &raw_url,
        ]);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(dur) = txt.parse::<f64>() {
                    if dur.is_finite() && dur > 0.0 {
                        return Ok(dur);
                    }
                }
            }
        }
        Err("无法获取视频时长".into())
    })
    .await
    .map_err(|e| format!("查询时长异常: {}", e))?
}

#[tauri::command]
async fn start_ffmpeg(url: String, seek_sec: Option<f64>, state: State<'_, AppState>) -> Result<String, String> {
    let my_gen = state.ffmpeg_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let proc_arc = state.ffmpeg_process.clone();
    let op_lock_arc = state.ffmpeg_op_lock.clone();
    let gen_arc = state.ffmpeg_generation.clone();
    let bili_cookie = state.bili_cookie.lock().unwrap().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let hls_dir = "/tmp/cooltv_hls";
        let _ = fs::create_dir_all(hls_dir);

        {
            let _op_guard = op_lock_arc.lock().unwrap();
            if gen_arc.load(Ordering::SeqCst) != my_gen {
                return Err("已切换新视频".into());
            }
            let old_child = {
                let mut process_guard = proc_arc.lock().unwrap();
                process_guard.take()
            };
            if let Some((_, mut child)) = old_child {
                let _ = child.kill();
                let _ = child.wait();
            }
            if let Ok(entries) = fs::read_dir(hls_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if name.ends_with(".ts") || name.ends_with(".m3u8") || name.ends_with(".tmp") {
                            let _ = fs::remove_file(p);
                        }
                    }
                }
            }
        }

        let (clean_url, _, _) = strip_stream_meta_suffix(&url);
        let is_bili_input = clean_url.starts_with("bilibili:")
            || clean_url.starts_with("douyin:bili:")
            || clean_url.starts_with("bili:")
            || clean_url.contains("bilivideo.")
            || clean_url.contains("hdslb.com")
            || clean_url.contains("szbdyd.com");

        // (cand_url, use_sw_fallback, skip_audio_filter, force_no_proxy)
        let mut attempts: Vec<(String, bool, bool, bool)> = Vec::new();
        let mut is_douyin = false;
        let mut is_bilibili = false;

        if is_bili_input {
            is_bilibili = true;
            let cands = if clean_url.starts_with("http://") || clean_url.starts_with("https://") {
                vec![clean_url.to_string()]
            } else {
                let (c, _) = resolve_bilibili_stream_candidates(clean_url, &bili_cookie);
                c
            };
            for c in cands {
                attempts.push((c.clone(), false, false, true));
                attempts.push((c.clone(), true, false, true));
            }
        } else if let Some(stripped) = clean_url.strip_prefix("douyin:h264:").or_else(|| clean_url.strip_prefix("douyin:")) {
            is_douyin = true;
            let list: Vec<String> = stripped
                .split("@@")
                .map(|s| s.trim().trim_start_matches("h264:"))
                .filter(|s| s.starts_with("http://") || s.starts_with("https://"))
                .map(|s| s.to_string())
                .collect();
            for c in list {
                attempts.push((c.clone(), false, false, true));
                attempts.push((c.clone(), true, false, true));
            }
        } else if clean_url.starts_with('/') {
            attempts.push((clean_url.to_string(), false, false, true));
            attempts.push((clean_url.to_string(), true, false, true));
        } else {
            let dy = clean_url.contains("douyin")
                || clean_url.contains("byte")
                || clean_url.contains("amemv")
                || clean_url.contains("zjcdn")
                || clean_url.contains("snssdk")
                || clean_url.contains("ixigua")
                || clean_url.contains("pstatp");
            if dy {
                is_douyin = true;
                for s in clean_url.split("@@").map(|x| x.trim().trim_start_matches("h264:")).filter(|s| s.starts_with("http://") || s.starts_with("https://")) {
                    let c = s.to_string();
                    attempts.push((c.clone(), false, false, true));
                    attempts.push((c.clone(), true, false, true));
                }
            } else {
                let iptv_cands = resolve_iptv_stream_candidates(&url);
                for (c, no_proxy) in iptv_cands {
                    attempts.push((c.clone(), false, false, no_proxy));
                    attempts.push((c.clone(), true, true, no_proxy));
                }
            }
        }

        if attempts.is_empty() {
            return Err("无效的视频地址".into());
        }

        let ffmpeg_bin = get_ffmpeg_bin();

        for (attempt_idx, (cand_url, use_sw, skip_af, force_no_proxy)) in attempts.iter().enumerate() {
            if gen_arc.load(Ordering::SeqCst) != my_gen {
                return Err("已切换新视频".into());
            }

            let session_id = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            let m3u8_filename = format!("stream_{}_{}_{}.m3u8", session_id, my_gen, attempt_idx);
            let m3u8_path = format!("{}/{}", hls_dir, m3u8_filename);
            let ts_pattern = format!("{}/seg_{}_{}_{}_%04d.ts", hls_dir, session_id, my_gen, attempt_idx);
            let is_local_file = cand_url.starts_with('/');

            let args = build_ffmpeg_args(
                cand_url,
                is_local_file,
                is_douyin,
                is_bilibili,
                *use_sw,
                *skip_af,
                seek_sec,
                &ts_pattern,
                &m3u8_path,
            );

            let spawn_res = {
                let _op_guard = op_lock_arc.lock().unwrap();
                if gen_arc.load(Ordering::SeqCst) != my_gen {
                    return Err("已切换新视频".into());
                }
                let old_child = {
                    let mut process_guard = proc_arc.lock().unwrap();
                    process_guard.take()
                };
                if let Some((_, mut old_c)) = old_child {
                    let _ = old_c.kill();
                    let _ = old_c.wait();
                }
                let mut cmd = Command::new(ffmpeg_bin);
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if *force_no_proxy {
                    apply_no_proxy_env(&mut cmd);
                }
                match cmd.args(&args).spawn() {
                    Ok(c) => {
                        let mut process_guard = proc_arc.lock().unwrap();
                        *process_guard = Some((my_gen, c));
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            };

            if let Err(e) = spawn_res {
                if attempt_idx + 1 == attempts.len() {
                    return Err(format!("FFmpeg 启动失败: {}", e));
                }
                continue;
            }

            let max_ticks = if attempt_idx + 1 < attempts.len() { 54 } else { 96 };
            let mut exited_early = false;

            for tick_idx in 0..max_ticks {
                if gen_arc.load(Ordering::SeqCst) != my_gen {
                    return Err("已切换新视频".into());
                }
                {
                    let mut process_guard = proc_arc.lock().unwrap();
                    if let Some((g, c)) = process_guard.as_mut() {
                        if *g == my_gen {
                            if let Ok(Some(status)) = c.try_wait() {
                                if status.success() {
                                    if let Ok(content) = fs::read_to_string(&m3u8_path) {
                                        if content.contains(".ts") {
                                            return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                                        }
                                    }
                                }
                                exited_early = true;
                            }
                        } else {
                            return Err("已切换新视频".into());
                        }
                    }
                }
                if exited_early {
                    break;
                }
                if let Ok(content) = fs::read_to_string(&m3u8_path) {
                    if is_local_file || is_douyin || is_bilibili {
                        // 任务 1 (Round 25) & 任务 6 (Round 31)：本地媒体与短视频均预缓冲至少 3 个完整 1s 切片 (_0002.ts) 或已写入 #EXT-X-ENDLIST 后再挂载播放器，防止首片边界缓冲区欠载触发 AVSampleBufferDisplayLayer 画面冻结
                        if content.contains("_0002.ts") || content.contains("#EXT-X-ENDLIST") || (tick_idx >= 26 && content.contains("_0001.ts")) {
                            return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                        }
                    } else if content.contains(".ts") {
                        return Ok(format!("http://127.0.0.1:8765/{}", m3u8_filename));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(85));
            }

            let stale_child = {
                let mut process_guard = proc_arc.lock().unwrap();
                if let Some((g, _)) = process_guard.as_ref() {
                    if *g == my_gen {
                        process_guard.take()
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some((_, mut c)) = stale_child {
                let _ = c.kill();
                let _ = c.wait();
            }
            let _ = fs::remove_file(&m3u8_path);
        }

        Err("转码等待超时".into())
    })
    .await
    .map_err(|e| format!("任务执行异常: {}", e))?
}

#[tauri::command]
async fn scan_local_folder(path: String) -> Result<Vec<(String, String)>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = Path::new(&path);
        if !root.exists() {
            return Err(format!("路径不存在: {}", path));
        }

        let valid_exts = [
            "mp4", "mkv", "mov", "avi", "ts", "m4v", "webm", "flv", "wmv",
            "rmvb", "rm", "mpg", "mpeg", "vob", "3gp", "asf", "divx", "f4v", "m2ts", "mts", "ogv", "dat"
        ];
        let mut results = Vec::new();

        fn visit_dirs(dir: &Path, exts: &[&str], out: &mut Vec<(String, String)>, depth: usize) {
            if depth > 4 { return; }
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        visit_dirs(&p, exts, out, depth + 1);
                    } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if exts.contains(&ext.to_lowercase().as_str()) {
                            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                                if !stem.starts_with('.') {
                                    out.push((stem.to_string(), p.to_string_lossy().to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        visit_dirs(root, &valid_exts, &mut results, 0);
        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(results)
    })
    .await
    .map_err(|e| format!("扫描异常: {}", e))?
}

#[tauri::command]
async fn extract_video_thumb(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let thumb_dir = "/tmp/cooltv_hls/thumbs";
    let _ = fs::create_dir_all(thumb_dir);

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash_id = hasher.finish();
    let filename = format!("thumb_{}.jpg", hash_id);
    let full_out = format!("{}/{}", thumb_dir, filename);

    let is_valid_jpg = |p: &str| -> bool {
        fs::metadata(p).map(|m| m.len() > 256).unwrap_or(false)
    };

    if is_valid_jpg(&full_out) {
        return Ok(format!("http://127.0.0.1:8765/thumbs/{}", filename));
    }

    let lock_arc = state.thumb_lock.clone();
    let full_out_clone = full_out.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let _guard = lock_arc.lock().unwrap();
        if !is_valid_jpg(&full_out_clone) {
            let _ = fs::remove_file(&full_out_clone);
            let ffmpeg_bin = get_ffmpeg_bin();
            for seek_sec in &["5", "1", "0"] {
                let _ = Command::new(ffmpeg_bin)
                    .args(&[
                        "-y",
                        "-v", "error",
                        "-fflags", "+genpts+igndts+discardcorrupt",
                        "-threads", "2",
                        "-ss", seek_sec,
                        "-i", &path,
                        "-frames:v", "1",
                        "-vf", "scale=480:-2",
                        "-q:v", "4",
                        &full_out_clone,
                    ])
                    .status();
                if is_valid_jpg(&full_out_clone) {
                    break;
                }
                let _ = fs::remove_file(&full_out_clone);
            }
        }
        if is_valid_jpg(&full_out_clone) {
            Ok(format!("http://127.0.0.1:8765/thumbs/{}", filename))
        } else {
            Err("截帧失败".into())
        }
    })
    .await
    .map_err(|e| format!("截帧异常: {}", e))?
}

fn extract_first_rating(text: &str) -> Option<String> {
    let marker = "rating_nums\">";
    if let Some(pos) = text.find(marker) {
        let rest = &text[pos + marker.len()..];
        if let Some(end) = rest.find('<') {
            let score = rest[..end].trim();
            if let Ok(val) = score.parse::<f32>() {
                if val > 0.0 && val <= 10.0 {
                    return Some(format!("{:.1}", val));
                }
            }
        }
    }
    let mut search_slice = text;
    while let Some(pos) = search_slice.find("\"value\":") {
        let after = &search_slice[pos + 8..];
        let num_str: String = after
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(val) = num_str.parse::<f32>() {
            if val > 0.0 && val <= 10.0 {
                return Some(format!("{:.1}", val));
            }
        }
        search_slice = after;
    }
    None
}

fn url_encode_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[tauri::command]
async fn fetch_douban_rating(title: String) -> Result<String, String> {
    let clean = title.trim().to_string();
    if clean.is_empty() {
        return Err("标题为空".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let encoded = url_encode_simple(&clean);
        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

        let urls = [
            (
                format!("https://www.douban.com/j/search?q={}&cat=1002", encoded),
                "https://www.douban.com/search",
            ),
            (
                format!("https://m.douban.com/rexxar/api/v2/search?q={}&type=movie", encoded),
                "https://m.douban.com/movie/",
            ),
            (
                format!("https://www.douban.com/search?cat=1002&q={}", encoded),
                "https://www.douban.com/",
            ),
        ];

        for (u, referer) in &urls {
            if let Ok(output) = Command::new("/usr/bin/curl")
                .args(&[
                    "-sL",
                    "--noproxy",
                    "*",
                    "--compressed",
                    "--max-time",
                    "6",
                    "-H",
                    &format!("User-Agent: {}", ua),
                    "-H",
                    &format!("Referer: {}", referer),
                    u,
                ])
                .output()
            {
                if output.status.success() {
                    let body = String::from_utf8_lossy(&output.stdout);
                    if let Some(score) = extract_first_rating(&body) {
                        return Ok(score);
                    }
                }
            }
        }
        Err("未找到豆瓣评分".into())
    })
    .await
    .map_err(|e| format!("查询异常: {}", e))?
}

fn parse_xmltv_timestamp(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if trimmed.len() < 14 {
        return None;
    }
    let digits = &trimmed[0..14];
    let year: i64 = digits[0..4].parse().ok()?;
    let month: i64 = digits[4..6].parse().ok()?;
    let day: i64 = digits[6..8].parse().ok()?;
    let hour: i64 = digits[8..10].parse().ok()?;
    let min: i64 = digits[10..12].parse().ok()?;
    let sec: i64 = digits[12..14].parse().ok()?;

    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m_adj = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * m_adj + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe - 719468
    }

    let days = days_from_civil(year, month, day);
    let mut epoch = days * 86400 + hour * 3600 + min * 60 + sec;

    let rest = trimmed[14..].trim();
    if rest.len() >= 5 && (rest.starts_with('+') || rest.starts_with('-')) {
        let sign: i64 = if rest.starts_with('+') { 1 } else { -1 };
        let tz_h: i64 = rest[1..3].parse().unwrap_or(8);
        let tz_m: i64 = rest[3..5].parse().unwrap_or(0);
        epoch -= sign * (tz_h * 3600 + tz_m * 60);
    } else {
        epoch -= 8 * 3600;
    }

    Some(epoch)
}

fn parse_xmltv_sec_of_day(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    if trimmed.len() < 14 {
        return None;
    }
    let digits = &trimmed[0..14];
    let hour: i64 = digits[8..10].parse().ok()?;
    let min: i64 = digits[10..12].parse().ok()?;
    let sec: i64 = digits[12..14].parse().ok()?;
    Some(hour * 3600 + min * 60 + sec)
}

fn normalize_channel_key(name: &str) -> String {
    name.trim()
        .to_uppercase()
        .replace('-', "")
        .replace('_', "")
        .replace(' ', "")
        .replace("综合", "")
        .replace("财经", "")
        .replace("纪录", "")
        .replace("新闻", "")
        .replace("频道", "")
        .replace("高清", "")
        .replace("FHD", "")
        .replace("HD", "")
        .replace("4K", "")
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("<![CDATA[", "")
        .replace("]]>", "")
}

fn extract_attr<'a>(tag: &'a str, attr_name: &str) -> Option<&'a str> {
    let pattern = format!("{}=\"", attr_name);
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn parse_epg_body_into_map(body: &str, result_map: &mut HashMap<String, String>) {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        if let Ok(json_map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
            for (k, v) in json_map {
                if let Some(s) = v.as_str() {
                    if !s.trim().is_empty() {
                        result_map.insert(k.clone(), s.to_string());
                        result_map.insert(normalize_channel_key(&k), s.to_string());
                    }
                }
            }
        }
        return;
    }

    let now_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let now_bj_sec_of_day = (now_epoch + 8 * 3600).rem_euclid(86400);

    let mut chan_id_to_names: HashMap<String, Vec<String>> = HashMap::new();
    let mut tod_fallback_map: HashMap<String, String> = HashMap::new();

    let mut search_pos = 0;
    while let Some(c_pos) = body[search_pos..].find("<channel") {
        let abs_start = search_pos + c_pos;
        if let Some(c_end) = body[abs_start..].find("</channel>") {
            let block = &body[abs_start..abs_start + c_end];
            if let Some(tag_end) = block.find('>') {
                let open_tag = &block[..tag_end];
                if let Some(cid) = extract_attr(open_tag, "id") {
                    let mut names = vec![cid.to_string()];
                    let mut dn_pos = tag_end;
                    while let Some(d_start) = block[dn_pos..].find("<display-name") {
                        let d_abs = dn_pos + d_start;
                        if let Some(gt) = block[d_abs..].find('>') {
                            let content_start = d_abs + gt + 1;
                            if let Some(lt) = block[content_start..].find("</display-name>") {
                                let dn = unescape_xml(block[content_start..content_start + lt].trim());
                                if !dn.is_empty() && !names.contains(&dn) {
                                    names.push(dn);
                                }
                                dn_pos = content_start + lt + 15;
                                continue;
                            }
                        }
                        break;
                    }
                    chan_id_to_names.insert(cid.to_string(), names);
                }
            }
            search_pos = abs_start + c_end + 10;
        } else {
            break;
        }
    }

    let mut prog_pos = 0;
    while let Some(p_pos) = body[prog_pos..].find("<programme") {
        let abs_start = prog_pos + p_pos;
        if let Some(p_end) = body[abs_start..].find("</programme>") {
            let block = &body[abs_start..abs_start + p_end];
            if let Some(tag_end) = block.find('>') {
                let open_tag = &block[..tag_end];
                if let (Some(start_str), Some(stop_str), Some(cid)) = (
                    extract_attr(open_tag, "start"),
                    extract_attr(open_tag, "stop"),
                    extract_attr(open_tag, "channel"),
                ) {
                    let mut matched_exact = false;
                    let mut matched_tod = false;

                    if let (Some(st), Some(et)) = (parse_xmltv_timestamp(start_str), parse_xmltv_timestamp(stop_str)) {
                        if st <= now_epoch && now_epoch < et {
                            matched_exact = true;
                        }
                    }
                    if !matched_exact {
                        if let (Some(s_tod), Some(e_tod)) = (parse_xmltv_sec_of_day(start_str), parse_xmltv_sec_of_day(stop_str)) {
                            if s_tod <= e_tod {
                                if s_tod <= now_bj_sec_of_day && now_bj_sec_of_day < e_tod {
                                    matched_tod = true;
                                }
                            } else if now_bj_sec_of_day >= s_tod || now_bj_sec_of_day < e_tod {
                                matched_tod = true;
                            }
                        }
                    }

                    if matched_exact || matched_tod {
                        if let Some(t_start) = block[tag_end..].find("<title") {
                            let t_abs = tag_end + t_start;
                            if let Some(gt) = block[t_abs..].find('>') {
                                let c_start = t_abs + gt + 1;
                                if let Some(lt) = block[c_start..].find("</title>") {
                                    let prog_title = unescape_xml(block[c_start..c_start + lt].trim());
                                    if !prog_title.is_empty() {
                                        let target_ref = if matched_exact { &mut *result_map } else { &mut tod_fallback_map };
                                        target_ref.insert(cid.to_string(), prog_title.clone());
                                        target_ref.insert(normalize_channel_key(cid), prog_title.clone());
                                        if let Some(names) = chan_id_to_names.get(cid) {
                                            for n in names {
                                                target_ref.insert(n.clone(), prog_title.clone());
                                                target_ref.insert(normalize_channel_key(n), prog_title.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            prog_pos = abs_start + p_end + 12;
        } else {
            break;
        }
    }

    for (k, v) in tod_fallback_map {
        result_map.entry(k).or_insert(v);
    }
}

#[tauri::command]
async fn fetch_epg_data(epg_url: String) -> Result<HashMap<String, String>, String> {
    let primary_url = if epg_url.trim().is_empty() {
        "https://epg.51zmt.top:8000/e.xml".to_string()
    } else {
        epg_url.trim().to_string()
    };

    tauri::async_runtime::spawn_blocking(move || {
        let mut result_map: HashMap<String, String> = HashMap::new();

        let mut urls_to_try = vec![primary_url.clone()];
        if primary_url.starts_with("https://") && primary_url.contains(":8000") {
            urls_to_try.push(primary_url.replacen("https://", "http://", 1));
        }
        urls_to_try.push("https://live.fanmingming.com/e.xml".to_string());
        urls_to_try.push("https://raw.githubusercontent.com/cohirogaobo/feelcool-tv-config/refs/heads/main/epg_lite.json".to_string());

        for u in &urls_to_try {
            let mut curl_cmd = Command::new("/usr/bin/curl");
            curl_cmd.args(&[
                "-sL",
                "--compressed",
                "-k",
                "--max-time",
                "16",
                "-H",
                "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
            ]);
            if u.contains("51zmt") || u.contains("fanmingming") {
                curl_cmd.args(&["--noproxy", "*"]);
            }
            curl_cmd.arg(u);

            if let Ok(output) = curl_cmd.output() {
                if output.status.success() && output.stdout.len() > 20 {
                    let body = String::from_utf8_lossy(&output.stdout);
                    parse_epg_body_into_map(&body, &mut result_map);
                    if result_map.len() >= 15 {
                        break;
                    }
                }
            }
        }

        if result_map.is_empty() {
            Err("未能从 EPG 源解析到有效节目单".into())
        } else {
            Ok(result_map)
        }
    })
    .await
    .map_err(|e| format!("EPG 解析异常: {}", e))?
}

#[derive(serde::Serialize)]
struct WeatherBroadcastPayload {
    full_line: String,
    routine_headline: String,
    warning_headline: Option<String>,
    warning_key: String,
    areas: String,
    agency: String,
}

fn curl_get_json_direct(url: &str) -> (u16, Option<serde_json::Value>) {
    let out = Command::new("/usr/bin/curl")
        .args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--max-time",
            "6",
            "-H",
            "Accept-Encoding: gzip,deflate",
            "-H",
            "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15",
            "-w",
            "\n__HTTP_CODE__:%{http_code}",
            url,
        ])
        .output();

    if let Ok(o) = out {
        let text = String::from_utf8_lossy(&o.stdout);
        if let Some(idx) = text.rfind("\n__HTTP_CODE__:") {
            let body = &text[..idx];
            let code_str = &text[idx + 15..].trim();
            let code = code_str.parse::<u16>().unwrap_or(0);
            let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
            return (code, parsed);
        }
    }
    (0, None)
}

/// 去除区划名称末尾的“区”、“县”、“市”，如“玄武区”->“玄武”，“雨花台区”->“雨花台”
fn strip_district_suffix(name: &str) -> String {
    let t = name.trim();
    if t.chars().count() >= 2 {
        for suffix in &["新区", "区", "县", "市"] {
            if let Some(s) = t.strip_suffix(suffix) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    t.to_string()
}

/// 规范化街道名称，确保带有“街道”或“镇”后缀（如“梅山” -> “梅山街道”，“梅山街道” -> “梅山街道”）
fn normalize_street_name(name: &str) -> String {
    let t = name.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.ends_with("街道") || t.ends_with("镇") || t.ends_with("乡") {
        return t.to_string();
    }
    for suffix in &["办事处", "片区", "商圈", "地区"] {
        if let Some(s) = t.strip_suffix(suffix) {
            if !s.is_empty() {
                if s.ends_with("街道") || s.ends_with("镇") {
                    return s.to_string();
                }
                return format!("{}街道", s);
            }
        }
    }
    format!("{}街道", t)
}

/// 任务 6 (Round 10)：根据当前城市、区县、街道或经纬度，匹配当前位置所在街道与邻近的三个街道（共 4 个街道名称）
/// 例如南京市雨花台区梅山街道 -> “梅山街道 赛虹桥街道 铁心桥街道 西善桥街道”
fn resolve_four_nearby_streets(
    city: &str,
    current_dist: &str,
    current_street: &str,
    lat: Option<f64>,
    lon: Option<f64>,
) -> Vec<String> {
    let clean_city = strip_district_suffix(city);
    let clean_dist = strip_district_suffix(current_dist);
    let mut norm_street = normalize_street_name(current_street);

    // 南京各街道经纬度中心点（当定位接口仅返回坐标或区名时，可根据坐标精准匹配所在街道）
    let nanjing_street_coords: &[(&str, &str, f64, f64)] = &[
        ("雨花台", "梅山街道", 31.946, 118.672),
        ("雨花台", "西善桥街道", 31.966, 118.705),
        ("雨花台", "铁心桥街道", 31.972, 118.756),
        ("雨花台", "赛虹桥街道", 32.002, 118.758),
        ("雨花台", "雨花街道", 31.996, 118.784),
        ("雨花台", "板桥街道", 31.918, 118.662),
        ("雨花台", "古雄街道", 31.902, 118.636),
        ("玄武", "新街口街道", 32.048, 118.788),
        ("玄武", "玄武门街道", 32.071, 118.785),
        ("玄武", "梅园新村街道", 32.044, 118.805),
        ("玄武", "锁金村街道", 32.078, 118.812),
        ("玄武", "孝陵卫街道", 32.036, 118.858),
        ("玄武", "红山街道", 32.092, 118.806),
        ("秦淮", "夫子庙街道", 32.022, 118.788),
        ("秦淮", "五老村街道", 32.038, 118.795),
        ("秦淮", "洪武路街道", 32.031, 118.786),
        ("秦淮", "朝天宫街道", 32.035, 118.774),
        ("秦淮", "中华门街道", 32.009, 118.779),
        ("建邺", "兴隆街道", 32.018, 118.736),
        ("建邺", "莫愁湖街道", 32.034, 118.756),
        ("建邺", "南苑街道", 32.016, 118.752),
        ("建邺", "沙洲街道", 31.992, 118.725),
        ("鼓楼", "湖南路街道", 32.068, 118.776),
        ("鼓楼", "华侨路街道", 32.052, 118.774),
        ("鼓楼", "宁海路街道", 32.062, 118.765),
        ("鼓楼", "江东街道", 32.048, 118.738),
        ("栖霞", "仙林街道", 32.105, 118.926),
        ("栖霞", "马群街道", 32.054, 118.902),
        ("栖霞", "迈皋桥街道", 32.104, 118.816),
        ("栖霞", "尧化街道", 32.122, 118.868),
        ("江宁", "东山街道", 31.954, 118.842),
        ("江宁", "秣陵街道", 31.922, 118.818),
        ("江宁", "淳化街道", 31.942, 118.928),
        ("江宁", "麒麟街道", 32.024, 118.924),
        ("浦口", "江浦街道", 32.058, 118.628),
        ("浦口", "顶山街道", 32.105, 118.675),
        ("浦口", "泰山街道", 32.142, 118.712),
    ];

    if norm_street.is_empty() && clean_city == "南京" {
        if let (Some(la), Some(lo)) = (lat, lon) {
            let mut best_st = "";
            let mut best_dist_sq = f64::MAX;
            for (d_name, st_name, sla, slo) in nanjing_street_coords {
                if !clean_dist.is_empty() && *d_name != clean_dist {
                    continue;
                }
                let dsq = (la - sla).powi(2) + (lo - slo).powi(2);
                if dsq < best_dist_sq {
                    best_dist_sq = dsq;
                    best_st = st_name;
                }
            }
            if !best_st.is_empty() {
                norm_street = best_st.to_string();
            }
        }
    }

    // 任务 1 (Round 15)：原气象速报共显示四个街道，现在起改为只显示三个街道
    let yuhuatai_map: &[(&str, &[&str])] = &[
        ("梅山街道", &["梅山街道", "赛虹桥街道", "铁心桥街道"]),
        ("赛虹桥街道", &["赛虹桥街道", "雨花街道", "铁心桥街道"]),
        ("铁心桥街道", &["铁心桥街道", "赛虹桥街道", "雨花街道"]),
        ("西善桥街道", &["西善桥街道", "梅山街道", "赛虹桥街道"]),
        ("雨花街道", &["雨花街道", "赛虹桥街道", "铁心桥街道"]),
        ("板桥街道", &["板桥街道", "古雄街道", "梅山街道"]),
        ("古雄街道", &["古雄街道", "板桥街道", "梅山街道"]),
    ];

    if clean_dist == "雨花台" || norm_street == "梅山街道" || norm_street == "赛虹桥街道" || norm_street == "铁心桥街道" || norm_street == "西善桥街道" {
        for (k, list) in yuhuatai_map {
            if *k == norm_street {
                return list.iter().take(3).map(|s| s.to_string()).collect();
            }
        }
        return vec![
            "梅山街道".into(),
            "赛虹桥街道".into(),
            "铁心桥街道".into(),
        ];
    }

    // 各区县街道池：当前街道置顶，自动补齐同区邻近两个街道（共显示三个街道）
    let district_street_pools: &[(&str, &str, &[&str])] = &[
        ("南京", "玄武", &["新街口街道", "玄武门街道", "梅园新村街道", "锁金村街道", "孝陵卫街道", "红山街道", "玄武湖街道"]),
        ("南京", "秦淮", &["夫子庙街道", "五老村街道", "洪武路街道", "朝天宫街道", "中华门街道", "双塘街道", "月牙湖街道", "光华路街道"]),
        ("南京", "建邺", &["兴隆街道", "莫愁湖街道", "南苑街道", "沙洲街道", "双闸街道", "江心洲街道"]),
        ("南京", "鼓楼", &["湖南路街道", "华侨路街道", "宁海路街道", "中央门街道", "挹江门街道", "江东街道", "凤凰街道", "热河南路街道"]),
        ("南京", "栖霞", &["仙林街道", "马群街道", "迈皋桥街道", "尧化街道", "燕子矶街道", "栖霞街道", "龙潭街道", "西岗街道"]),
        ("南京", "雨花台", &["梅山街道", "赛虹桥街道", "铁心桥街道", "西善桥街道", "雨花街道", "板桥街道", "古雄街道"]),
        ("南京", "江宁", &["东山街道", "秣陵街道", "百家湖街道", "淳化街道", "麒麟街道", "江宁街道", "汤山街道", "禄口街道"]),
        ("南京", "浦口", &["江浦街道", "顶山街道", "泰山街道", "沿江街道", "桥林街道", "汤泉街道"]),
        ("南京", "六合", &["雄州街道", "龙池街道", "横梁街道", "金牛湖街道"]),
        ("南京", "溧水", &["永阳街道", "柘塘街道", "东屏街道", "石湫街道"]),
        ("南京", "高淳", &["淳溪街道", "古柏街道", "漆桥街道", "固城街道"]),
        ("北京", "朝阳", &["建外街道", "朝外街道", "呼家楼街道", "三里屯街道", "望京街道", "奥园街道"]),
        ("北京", "海淀", &["中关村街道", "海淀街道", "学院路街道", "万柳街道", "清河街道", "羊坊店街道"]),
        ("北京", "东城", &["东华门街道", "景山街道", "交道口街道", "安定门街道", "北新桥街道"]),
        ("北京", "西城", &["西长安街街道", "新街口街道", "月坛街道", "金融街街道", "什刹海街道"]),
        ("上海", "浦东", &["陆家嘴街道", "惟明街道", "塘桥街道", "洋泾街道", "花木街道", "张江镇"]),
        ("上海", "黄浦", &["外滩街道", "南京东路街道", "淮海中路街道", "瑞金二路街道", "豫园街道"]),
        ("上海", "徐汇", &["徐家汇街道", "天平路街道", "湖南路街道", "枫林路街道", "漕河泾街道"]),
        ("上海", "静安", &["静安寺街道", "南京西路街道", "江宁路街道", "曹家渡街道", "天目西路街道"]),
        ("广州", "天河", &["天河南街道", "林和街道", "猎德街道", "冼村街道", "石牌街道", "五山街道"]),
        ("广州", "越秀", &["北京街道", "光塔街道", "人民街道", "东山街道", "农林街道"]),
        ("深圳", "南山", &["粤海街道", "南头街道", "南山街道", "沙河街道", "蛇口街道", "西丽街道"]),
        ("深圳", "福田", &["福田街道", "华强北街道", "园岭街道", "南园街道", "沙头街道", "莲花街道"]),
        ("杭州", "西湖", &["北山街道", "西溪街道", "灵隐街道", "翠苑街道", "文新街道", "古荡街道"]),
        ("杭州", "上城", &["湖滨街道", "清波街道", "小营街道", "望江街道", "四季青街道"]),
        ("苏州", "姑苏", &["平江街道", "金阊街道", "沧浪街道", "双塔街道", "虎丘街道"]),
        ("无锡", "梁溪", &["崇安寺街道", "通江街道", "广瑞路街道", "迎龙桥街道", "南禅寺街道"]),
        ("常州", "天宁", &["雕庄街道", "青龙街道", "茶山街道", "红梅街道", "天宁街道"]),
        ("成都", "锦江", &["春熙路街道", "书院街街道", "合江亭街道", "牛市口街道", "沙河街道"]),
        ("武汉", "武昌", &["黄鹤楼街道", "粮道街街道", "中华路街道", "水果湖街道", "中南路街道"]),
        ("西安", "雁塔", &["小寨路街道", "大雁塔街道", "长延堡街道", "电子城街道", "曲江街道"]),
    ];

    let mut out = Vec::new();
    if !norm_street.is_empty() {
        out.push(norm_street.clone());
    }

    for (c_name, d_name, pool) in district_street_pools {
        if (*c_name == clean_city && *d_name == clean_dist)
            || (!norm_street.is_empty() && pool.iter().any(|s| *s == norm_street))
        {
            for st in *pool {
                if out.len() >= 3 {
                    break;
                }
                let st_str = st.to_string();
                if !out.contains(&st_str) {
                    out.push(st_str);
                }
            }
            break;
        }
    }

    if out.len() < 3 && clean_city == "南京" {
        for fallback_st in &["梅山街道", "赛虹桥街道", "铁心桥街道"] {
            if out.len() >= 3 {
                break;
            }
            let s = fallback_st.to_string();
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }

    if out.len() < 3 {
        let base = if !clean_dist.is_empty() { &clean_dist } else if !clean_city.is_empty() { &clean_city } else { "中心" };
        let generic_fallbacks = [
            format!("{}街道", base),
            "新华街道".to_string(),
            "朝阳街道".to_string(),
        ];
        for g in &generic_fallbacks {
            if out.len() >= 3 {
                break;
            }
            if !out.contains(g) {
                out.push(g.clone());
            }
        }
    }

    out.into_iter().take(3).collect()
}

#[tauri::command]
async fn fetch_weather_broadcast(
    client_lat: Option<f64>,
    client_lon: Option<f64>,
    location_mode: Option<String>,
    manual_city: Option<String>,
    manual_district: Option<String>,
    manual_street: Option<String>,
    state: State<'_, AppState>,
) -> Result<WeatherBroadcastPayload, String> {
    let mc_400_arc = state.minutecast_400_until.clone();
    let was_precip_arc = state.was_precipitating.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let api_key = "oLpVBp2pSM2J0ofeGZDdzfMPYePEbRZK";

        let is_china_coord = |la: f64, lo: f64| -> bool {
            la >= 16.0 && la <= 54.0 && lo >= 73.0 && lo <= 136.0
        };
        let is_china_country = |c: &str| -> bool {
            let t = c.trim();
            t.is_empty() || t == "中国" || t.eq_ignore_ascii_case("cn") || t.eq_ignore_ascii_case("china")
        };
        let is_foreign_place_name = |name: &str| -> bool {
            let t = name.trim();
            if t.is_empty() {
                return false;
            }
            let foreign_keywords = [
                "阿姆斯特丹", "荷兰", "美国", "弗吉尼亚", "阿什本", "洛杉矶", "圣何塞",
                "新加坡", "东京", "日本", "首尔", "韩国", "伦敦", "英国", "法兰克福", "德国",
            ];
            foreign_keywords.iter().any(|kw| t.contains(kw))
        };

        let m_city = manual_city.unwrap_or_default().trim().to_string();
        let m_dist = manual_district.unwrap_or_default().trim().to_string();
        let m_street = manual_street.unwrap_or_default().trim().to_string();
        let is_manual = location_mode.as_deref() == Some("manual")
            && (!m_city.is_empty() || !m_dist.is_empty() || !m_street.is_empty());

        let mut lat: Option<f64> = None;
        let mut lon: Option<f64> = None;
        let mut domestic_city = String::new();
        let mut domestic_district = String::new();
        let mut domestic_province = String::new();
        let mut domestic_street = String::new();

        if is_manual {
            domestic_city = m_city.clone();
            domestic_district = m_dist.clone();
            domestic_street = m_street.clone();
        } else {
            if let (Some(clat), Some(clon)) = (client_lat, client_lon) {
                if is_china_coord(clat, clon) {
                    lat = Some(clat);
                    lon = Some(clon);
                }
            }

            // 强制直连调用美团高精度位置接口（严格校验中国境内坐标与国家，拦截 VPN 海外出口如阿姆斯特丹）
            let (_, mt_json) = curl_get_json_direct("https://apimobile.meituan.com/locate/v2/ip/loc?rgeo=true");
            if let Some(mj) = mt_json {
                if let Some(d) = mj.get("data") {
                    let rg_opt = d.get("rgeo");
                    let country_str = rg_opt
                        .and_then(|rg| rg.get("country"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let city_str = rg_opt
                        .and_then(|rg| rg.get("city"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let coord_ok = match (d.get("lat").and_then(|v| v.as_f64()), d.get("lng").and_then(|v| v.as_f64())) {
                        (Some(la), Some(lo)) => is_china_coord(la, lo),
                        _ => false,
                    };
                    if coord_ok && is_china_country(country_str) && !is_foreign_place_name(city_str) {
                        if lat.is_none() {
                            if let (Some(la), Some(lo)) = (d.get("lat").and_then(|v| v.as_f64()), d.get("lng").and_then(|v| v.as_f64())) {
                                lat = Some(la);
                                lon = Some(lo);
                            }
                        }
                        if let Some(rg) = rg_opt {
                            if !city_str.is_empty() {
                                domestic_city = city_str.to_string();
                            }
                            if let Some(dist) = rg.get("district").and_then(|v| v.as_str()) {
                                domestic_district = dist.to_string();
                            }
                            if let Some(prov) = rg.get("province").and_then(|v| v.as_str()) {
                                domestic_province = prov.to_string();
                            }
                            for st_key in &["street", "township", "town", "areaName"] {
                                if let Some(st) = rg.get(*st_key).and_then(|v| v.as_str()) {
                                    if !st.trim().is_empty() {
                                        domestic_street = st.trim().to_string();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 若有中国境内经纬度坐标，调用美团经纬度逆地理接口获取街道级定位信息
            if let (Some(la), Some(lo)) = (lat, lon) {
                if is_china_coord(la, lo) {
                    let mt_rev_url = format!(
                        "https://apimobile.meituan.com/group/v1/city/latlng/{:.5},{:.5}?tag=0",
                        la, lo
                    );
                    let (_, mt_rev_json) = curl_get_json_direct(&mt_rev_url);
                    if let Some(mrj) = mt_rev_json {
                        if let Some(d) = mrj.get("data") {
                            let c_name = d.get("city").and_then(|v| v.as_str()).unwrap_or("");
                            if !is_foreign_place_name(c_name) {
                                if domestic_city.is_empty() && !c_name.is_empty() {
                                    domestic_city = c_name.to_string();
                                }
                                if domestic_district.is_empty() {
                                    if let Some(dist) = d.get("district").and_then(|v| v.as_str()) {
                                        domestic_district = dist.to_string();
                                    }
                                }
                                if domestic_province.is_empty() {
                                    if let Some(prov) = d.get("province").and_then(|v| v.as_str()) {
                                        domestic_province = prov.to_string();
                                    }
                                }
                                if domestic_street.is_empty() {
                                    for st_key in &["areaName", "street", "township", "town"] {
                                        if let Some(st) = d.get(*st_key).and_then(|v| v.as_str()) {
                                            let st_trim = st.trim();
                                            if !st_trim.is_empty() && st_trim != domestic_district && st_trim != domestic_city {
                                                domestic_street = st_trim.to_string();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 备选直连：哔哩哔哩国内 IP 定位接口（严格校验中国境内）
            if lat.is_none() || domestic_city.is_empty() {
                let (_, bili_json) = curl_get_json_direct("https://api.live.bilibili.com/client/v1/Ip/getInfoNew");
                if let Some(bj) = bili_json {
                    if let Some(d) = bj.get("data") {
                        let country_str = d.get("country").and_then(|v| v.as_str()).unwrap_or("");
                        let city_str = d.get("city").and_then(|v| v.as_str()).unwrap_or("");
                        if is_china_country(country_str) && !is_foreign_place_name(city_str) {
                            if lat.is_none() {
                                let la_opt = d.get("latitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                                let lo_opt = d.get("longitude").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok())));
                                if let (Some(la), Some(lo)) = (la_opt, lo_opt) {
                                    if is_china_coord(la, lo) {
                                        lat = Some(la);
                                        lon = Some(lo);
                                    }
                                }
                            }
                            if domestic_city.is_empty() && !city_str.is_empty() {
                                domestic_city = city_str.to_string();
                            }
                            if domestic_province.is_empty() {
                                if let Some(p) = d.get("province").and_then(|v| v.as_str()) {
                                    if !is_foreign_place_name(p) {
                                        domestic_province = p.to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 备选直连：腾讯位置接口（严格校验 country == "中国"）
            if domestic_city.is_empty() {
                let (_, qq_json) = curl_get_json_direct("https://r.inews.qq.com/api/ip2city");
                if let Some(qj) = qq_json {
                    let country_str = qj.get("country").and_then(|v| v.as_str()).unwrap_or("");
                    let city_str = qj.get("city").and_then(|v| v.as_str()).unwrap_or("");
                    if is_china_country(country_str) && !is_foreign_place_name(city_str) && !city_str.is_empty() {
                        domestic_city = city_str.to_string();
                        if let Some(dist) = qj.get("district").and_then(|v| v.as_str()) {
                            if domestic_district.is_empty() {
                                domestic_district = dist.to_string();
                            }
                        }
                        if let Some(p) = qj.get("province").and_then(|v| v.as_str()) {
                            if domestic_province.is_empty() {
                                domestic_province = p.to_string();
                            }
                        }
                    }
                }
            }

            // 若因全局 VPN/TUN 代理导致所有 IP 定位均返回海外节点，优先回退用户保存的手动位置，否则回退默认国内城市（南京）
            if domestic_city.is_empty() && domestic_district.is_empty() && lat.is_none() {
                if !m_city.is_empty() || !m_dist.is_empty() || !m_street.is_empty() {
                    domestic_city = m_city;
                    domestic_district = m_dist;
                    domestic_street = m_street;
                } else {
                    domestic_city = "南京".to_string();
                    domestic_district = "雨花台区".to_string();
                    domestic_street = "梅山街道".to_string();
                }
            }
        }

        let mut loc_key = String::new();
        let mut admin_name = domestic_province.clone();

        if let (Some(la), Some(lo)) = (lat, lon) {
            if is_china_coord(la, lo) {
                let lat_round = (la * 1000.0).round() / 1000.0;
                let lon_round = (lo * 1000.0).round() / 1000.0;
                let q_coord = format!("{:.3},{:.3}", lat_round, lon_round);
                let geo_url = format!(
                    "https://openapi.weathercn.com/locations/v1/cities/geoposition/search.json?q={}&apikey={}&language=zh-cn&details=true",
                    q_coord, api_key
                );
                let (_, geo_json) = curl_get_json_direct(&geo_url);
                if let Some(gj) = geo_json {
                    let local_name = gj.get("LocalizedName").and_then(|v| v.as_str()).unwrap_or("");
                    if !is_foreign_place_name(local_name) {
                        if let Some(k) = gj.get("Key").and_then(|v| v.as_str()) {
                            loc_key = k.to_string();
                        }
                        if domestic_district.is_empty() && !local_name.is_empty() {
                            domestic_district = local_name.to_string();
                        }
                        if domestic_city.is_empty() {
                            if let Some(parent_name) = gj.get("ParentCity").and_then(|p| p.get("LocalizedName")).and_then(|v| v.as_str()) {
                                domestic_city = parent_name.to_string();
                            }
                        }
                        if let Some(adm) = gj.get("AdministrativeArea").and_then(|a| a.get("LocalizedName")).and_then(|v| v.as_str()) {
                            if !adm.is_empty() {
                                admin_name = adm.to_string();
                            }
                        }
                    }
                }
            }
        }

        if loc_key.is_empty() && (!domestic_district.is_empty() || !domestic_city.is_empty()) {
            let query_place = if !domestic_district.is_empty() { &domestic_district } else { &domestic_city };
            let trans_url = format!(
                "https://openapi.weathercn.com/locations/v1/cities/translate?q={}&apikey={}&language=zh-cn&details=true",
                url_encode_simple(query_place),
                api_key
            );
            let (_, trans_json) = curl_get_json_direct(&trans_url);
            if let Some(arr) = trans_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(first_hit) = arr.first() {
                    if let Some(k) = first_hit.get("Key").and_then(|v| v.as_str()) {
                        loc_key = k.to_string();
                    }
                    if let (Some(la), Some(lo)) = (
                        first_hit.get("GeoPosition").and_then(|g| g.get("Latitude")).and_then(|v| v.as_f64()),
                        first_hit.get("GeoPosition").and_then(|g| g.get("Longitude")).and_then(|v| v.as_f64()),
                    ) {
                        if is_china_coord(la, lo) {
                            lat = Some(la);
                            lon = Some(lo);
                        }
                    }
                    if domestic_city.is_empty() {
                        if let Some(parent_name) = first_hit.get("ParentCity").and_then(|p| p.get("LocalizedName")).and_then(|v| v.as_str()) {
                            domestic_city = parent_name.to_string();
                        } else if let Some(ln) = first_hit.get("LocalizedName").and_then(|v| v.as_str()) {
                            domestic_city = ln.to_string();
                        }
                    }
                    if let Some(adm) = first_hit.get("AdministrativeArea").and_then(|a| a.get("LocalizedName")).and_then(|v| v.as_str()) {
                        if !adm.is_empty() {
                            admin_name = adm.to_string();
                        }
                    }
                }
            }
        }

        // 任务 1 (Round 15)：原气象速报共显示四个街道，现在起改为只显示三个街道
        let three_streets = resolve_four_nearby_streets(
            &domestic_city,
            &domestic_district,
            &domestic_street,
            lat,
            lon,
        );
        let areas_display = three_streets.into_iter().take(3).collect::<Vec<_>>().join(" ");

        // 任务 6 (Round 10)：第二行居右气象台署名由省级改为市级（如“南京市气象台”）
        let clean_city_name = domestic_city
            .trim()
            .trim_end_matches("市")
            .trim_end_matches("地区")
            .trim_end_matches("自治州")
            .to_string();
        let agency_name = if !clean_city_name.is_empty() {
            format!("{}市气象台", clean_city_name)
        } else {
            let clean_admin = admin_name
                .trim_end_matches("省")
                .trim_end_matches("市")
                .trim_end_matches("壮族自治区")
                .trim_end_matches("回族自治区")
                .trim_end_matches("维吾尔自治区")
                .trim_end_matches("自治区")
                .trim_end_matches("特别行政区")
                .to_string();
            if !clean_admin.is_empty() {
                format!("{}市气象台", clean_admin)
            } else {
                "南京市气象台".to_string()
            }
        };

        let mut warning_headline: Option<String> = None;
        let mut warning_key = String::new();

        // 1. 查询灾害预警 API (/alerts/v1)
        if !loc_key.is_empty() {
            let alert_url = format!(
                "https://openapi.weathercn.com/alerts/v1/{}.json?apikey={}&language=zh-cn&details=true",
                loc_key, api_key
            );
            let (_, alert_json) = curl_get_json_direct(&alert_url);
            if let Some(arr) = alert_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(first_alert) = arr.first() {
                    let desc = first_alert
                        .get("Description")
                        .and_then(|d| d.get("Localized"))
                        .and_then(|v| v.as_str())
                        .or_else(|| first_alert.get("Type").and_then(|v| v.as_str()))
                        .unwrap_or("气象灾害预警");
                    let alert_id = first_alert.get("AlertID").and_then(|v| v.as_i64()).unwrap_or(1);
                    warning_headline = Some(format!("气象预警 {}", desc));
                    warning_key = format!("alert_{}", alert_id);
                }
            }
        }

        // 2. 查询分钟级降水 API (/nowcast_cn/v3/basic)，检测：降水开始、即将停止、降水结束
        let mut mc_if_pre = false;
        let mut mc_checked = false;
        let now_sec = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if let (Some(la), Some(lo)) = (lat, lon) {
            if now_sec >= mc_400_arc.load(Ordering::SeqCst) {
                let lat_round = (la * 1000.0).round() / 1000.0;
                let lon_round = (lo * 1000.0).round() / 1000.0;
                let q_coord = format!("{:.3},{:.3}", lat_round, lon_round);
                let mc_url = format!(
                    "https://openapi.weathercn.com/nowcast_cn/v3/basic?q={}&apikey={}&language=zh-cn",
                    q_coord, api_key
                );
                let (mc_code, mc_json) = curl_get_json_direct(&mc_url);
                if mc_code == 400 {
                    mc_400_arc.store(now_sec + 3600, Ordering::SeqCst);
                } else if let Some(mj) = mc_json {
                    if let Some(data_obj) = mj.get("Data") {
                        mc_checked = true;
                        mc_if_pre = data_obj.get("IfPre").and_then(|v| v.as_bool()).unwrap_or(false);
                        let short_phrase = data_obj
                            .get("ShortPhrase")
                            .and_then(|v| v.as_str())
                            .or_else(|| data_obj.get("Description").and_then(|v| v.as_str()))
                            .unwrap_or("");

                        let mut is_stopping_soon = short_phrase.contains("停") || short_phrase.contains("止");
                        if !is_stopping_soon {
                            if let Some(intens) = data_obj.get("Intensity").and_then(|v| v.as_array()) {
                                if intens.len() >= 6 {
                                    let first_val = intens[0].as_f64().unwrap_or(0.0);
                                    let tail_max = intens.iter().skip(intens.len() / 2).filter_map(|x| x.as_f64()).fold(0.0, f64::max);
                                    if first_val > 0.02 && tail_max == 0.0 {
                                        is_stopping_soon = true;
                                    }
                                }
                            }
                        }

                        if warning_headline.is_none() && mc_if_pre {
                            if is_stopping_soon {
                                let msg = if short_phrase.is_empty() { "降雨即将停止" } else { short_phrase };
                                warning_headline = Some(format!("降水预报 {}", msg));
                                warning_key = format!("mc_stopping_{}", msg);
                            } else {
                                let msg = if short_phrase.is_empty() { "即将开始降水" } else { short_phrase };
                                warning_headline = Some(format!("降水预报 {}", msg));
                                warning_key = format!("mc_starting_{}", msg);
                            }
                        }
                    }
                }
            }
        }

        // 3. 查询空气质量 API (/airquality/v1/global/observations)
        let mut aqi_val: i64 = 0;
        let mut pm25_val: f64 = 0.0;
        if !loc_key.is_empty() {
            let aq_url = format!(
                "https://openapi.weathercn.com/airquality/v1/global/observations/{}.json?apikey={}&language=zh-cn",
                loc_key, api_key
            );
            let (_, aq_json) = curl_get_json_direct(&aq_url);
            if let Some(aj) = aq_json {
                aqi_val = aj.get("Index").and_then(|v| v.as_i64()).unwrap_or(0);
                pm25_val = aj.get("ParticulateMatter2_5").and_then(|v| v.as_f64()).unwrap_or(0.0);
            }
        }

        // 4. 查询当前实况天气 API (/currentconditions/v1)
        let mut weather_text = String::from("多云");
        let mut temp_c: f64 = 20.0;
        let mut humidity: i64 = 55;
        let mut has_precip = false;

        if !loc_key.is_empty() {
            let cc_url = format!(
                "https://openapi.weathercn.com/currentconditions/v1/{}.json?apikey={}&language=zh-cn&details=true",
                loc_key, api_key
            );
            let (_, cc_json) = curl_get_json_direct(&cc_url);
            if let Some(arr) = cc_json.as_ref().and_then(|v| v.as_array()) {
                if let Some(cc) = arr.first() {
                    if let Some(wt) = cc.get("WeatherText").and_then(|v| v.as_str()) {
                        if !wt.is_empty() {
                            weather_text = wt.to_string();
                        }
                    }
                    if let Some(tc) = cc.get("Temperature").and_then(|t| t.get("Metric")).and_then(|m| m.get("Value")).and_then(|v| v.as_f64()) {
                        temp_c = tc;
                    }
                    if let Some(rh) = cc.get("RelativeHumidity").and_then(|v| v.as_i64()) {
                        humidity = rh;
                    }
                    has_precip = cc.get("HasPrecipitation").and_then(|v| v.as_bool()).unwrap_or(false);
                }
            }
        }

        // 检测分钟级降水结束（此前处于降水状态，当前 MinuteCast 与实况均无降水）
        let now_precip = mc_if_pre || has_precip;
        {
            let mut prev_p = was_precip_arc.lock().unwrap();
            if *prev_p && !now_precip && mc_checked && warning_headline.is_none() {
                warning_headline = Some("降水预报 降水已结束".to_string());
                warning_key = format!("mc_ended_{}", now_sec / 600);
            }
            *prev_p = now_precip;
        }

        // 检测 AQI 超标与湿度超标
        if warning_headline.is_none() && (aqi_val >= 101 || pm25_val >= 75.0) {
            warning_headline = Some(format!("空气质量提醒 AQI {}", aqi_val));
            warning_key = format!("aqi_over_{}", aqi_val / 20);
        }
        if warning_headline.is_none() && humidity >= 85 {
            warning_headline = Some(format!("高湿提醒 湿度{}%", humidity));
            warning_key = format!("humidity_over_{}", humidity / 5);
        }

        // 第一行居左常规格式：“气象速报 阴 22°C”
        let routine_headline = format!("气象速报 {} {:.0}°C", weather_text, temp_c);

        // 第三轮单行完整格式：“气象速报 阴 22°C 湿度69% AQI45   玄武区 南京 江苏省       江苏气象台 中国气象局”
        let aqi_part = if aqi_val > 0 { format!(" AQI{}", aqi_val) } else { String::new() };
        let base_status = if let Some(ref w) = warning_headline {
            w.clone()
        } else {
            format!("气象速报 {} {:.0}°C 湿度{}%{}", weather_text, temp_c, humidity, aqi_part)
        };
        let mut loc_tokens = Vec::new();
        if !domestic_district.is_empty() {
            loc_tokens.push(domestic_district.clone());
        }
        if !domestic_city.is_empty() && domestic_city != domestic_district {
            loc_tokens.push(domestic_city.clone());
        }
        if !admin_name.is_empty() && admin_name != domestic_city {
            loc_tokens.push(admin_name.clone());
        }
        let loc_str = if loc_tokens.is_empty() { areas_display.clone() } else { loc_tokens.join(" ") };
        let full_line = format!("{}   {}       {} 中国气象局", base_status, loc_str, agency_name);

        Ok(WeatherBroadcastPayload {
            full_line,
            routine_headline,
            warning_headline,
            warning_key,
            areas: areas_display,
            agency: agency_name,
        })
    })
    .await
    .map_err(|e| format!("天气获取异常: {}", e))?
}

fn url_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter().copied();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(b'0');
            let h2 = chars.next().unwrap_or(b'0');
            if let Ok(val) = u8::from_str_radix(&String::from_utf8_lossy(&[h1, h2]), 16) {
                bytes.push(val);
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn category_to_douyin_url(cat: &str) -> &'static str {
    match cat {
        "discover" => "https://www.douyin.com/discover",
        "recommend" => "https://www.douyin.com/?recommend=1",
        "knowledge" => "https://www.douyin.com/channel/300203",
        "acg" => "https://www.douyin.com/channel/300206",
        "game" => "https://www.douyin.com/channel/300205",
        "food" => "https://www.douyin.com/channel/300204",
        "sports" => "https://www.douyin.com/channel/300207",
        "fashion" => "https://www.douyin.com/channel/300208",
        "music" => "https://www.douyin.com/channel/300209",
        _ => "https://www.douyin.com/discover",
    }
}

fn ensure_douyin_scraper(
    app: &AppHandle,
    feed_arc: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
    probing_arc: Arc<Mutex<HashSet<String>>>,
    cat_arc: Arc<Mutex<String>>,
    probe_lock_arc: Arc<Mutex<()>>,
) {
    if app.get_webview_window("douyin_scraper").is_some() {
        return;
    }

    let hook_script = r#"
        (function() {
            // 任务 1 (Round 17)：突破 macOS 隐藏 WKWebView (visible: false) 后台休眠节流与尺寸检测
            // 在隐藏窗口中原生 requestAnimationFrame 会被 WebKit 挂起导致抖音 React/Pace 首屏长久卡死，改用 16ms 定时器驱动
            try {
                Object.defineProperty(window, 'innerWidth', { get: () => 1920, configurable: true });
                Object.defineProperty(window, 'innerHeight', { get: () => 1080, configurable: true });
                Object.defineProperty(window, 'outerWidth', { get: () => 1920, configurable: true });
                Object.defineProperty(window, 'outerHeight', { get: () => 1080, configurable: true });
                Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
                Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
                const origRaf = window.requestAnimationFrame;
                window.requestAnimationFrame = function(cb) {
                    if (document.getElementById('fc-login-bar') && origRaf) {
                        return origRaf.call(window, cb);
                    }
                    const delay = (Date.now() - (window.__FC_PAGE_START_TS__ || 0) > 4500) ? 48 : 16;
                    return setTimeout(() => {
                        try { cb(performance.now()); } catch (_) {}
                    }, delay);
                };
                window.cancelAnimationFrame = function(id) {
                    clearTimeout(id);
                };
            } catch(e) {}

            try {
                if (window.MediaSource && MediaSource.isTypeSupported) {
                    const origIsType = MediaSource.isTypeSupported.bind(MediaSource);
                    MediaSource.isTypeSupported = function(mime) {
                        if (typeof mime === 'string' && /hev1|hvc1|hevc|bytevc|bvc|av01|vp9/i.test(mime)) return false;
                        return origIsType(mime);
                    };
                }
                if (window.HTMLVideoElement && HTMLVideoElement.prototype && HTMLVideoElement.prototype.canPlayType) {
                    const origCanPlay = HTMLVideoElement.prototype.canPlayType;
                    HTMLVideoElement.prototype.canPlayType = function(mime) {
                        if (typeof mime === 'string' && /hev1|hvc1|hevc|bytevc|bvc|av01|vp9/i.test(mime)) return '';
                        return origCanPlay.apply(this, arguments);
                    };
                }
                if (window.MediaCapabilities && window.MediaCapabilities.prototype && window.MediaCapabilities.prototype.decodingInfo) {
                    const origDecodingInfo = window.MediaCapabilities.prototype.decodingInfo;
                    window.MediaCapabilities.prototype.decodingInfo = function(cfg) {
                        try {
                            const ct = (cfg && cfg.video && cfg.video.contentType) ? String(cfg.video.contentType) : '';
                            if (/hev1|hvc1|hevc|bytevc|bvc|av01|vp9/i.test(ct)) {
                                return Promise.resolve({ supported: false, smooth: false, powerEfficient: false });
                            }
                        } catch (_) {}
                        return origDecodingInfo.apply(this, arguments);
                    };
                }
                if (window.HTMLMediaElement && window.HTMLMediaElement.prototype) {
                    const origPlay = window.HTMLMediaElement.prototype.play;
                    window.HTMLMediaElement.prototype.play = function() {
                        if (!document.getElementById('fc-login-bar')) {
                            try {
                                this.muted = true;
                                this.pause();
                            } catch (_) {}
                            return Promise.resolve();
                        }
                        return origPlay.apply(this, arguments);
                    };
                }
            } catch(e) {}

            window.__FC_PAGE_START_TS__ = Date.now();
            window.__FC_LAST_LOAD_MORE_TS__ = 0;
            window.__FC_QUEUE__ = [];
            window.__FC_SEEN__ = new Set();
            window.__FC_AUTHOR_CACHE__ = {};

            function purgeDouyinFeedStorage() {
                try {
                    localStorage.removeItem('__fc_boot_seen_v1__');
                    try { sessionStorage.clear(); } catch (_) {}
                    for (let ki = localStorage.length - 1; ki >= 0; ki--) {
                        const k = localStorage.key(ki) || '';
                        if (/jingxuan|discover_feed|feed_cache|aweme_list|prefetch|recommend_cache|preload|water_fall/i.test(k)) {
                            localStorage.removeItem(k);
                        }
                    }
                } catch (_) {}
            }
            purgeDouyinFeedStorage();
            window.__FC_PURGE_STORAGE__ = purgeDouyinFeedStorage;

            setInterval(() => {
                if (window.__FC_QUEUE__.length > 0) {
                    const item = window.__FC_QUEUE__.shift();
                    const payload = item.map(s => encodeURIComponent(s)).join('|||');
                    try {
                        document.title = '__FC_ITEM__:' + payload + '|||' + Date.now();
                    } catch(e) {}
                }
            }, 35);

            function normalizeHttps(u) {
                if (!u || typeof u !== 'string') return '';
                u = u.trim();
                if (u.startsWith('//')) return 'https:' + u;
                if (u.startsWith('http://')) return 'https://' + u.substring(7);
                return u;
            }

            function isInvalidVideoUrl(u) {
                if (!u) return true;
                const lower = u.toLowerCase();
                if (lower.includes('mime_type=audio') || lower.includes('.mp3') || lower.includes('.m4a') || lower.includes('ies-music') || lower.includes('.mpd')) {
                    return true;
                }
                return false;
            }

            function isByteVC2Entry(br) {
                if (!br) return false;
                if (Number(br.is_bytevc1) === 2 || Number(br.is_h265) === 2) return true;
                const g = String(br.gear_name || '') + ' ' + String(br.codec_type || '');
                if (/bytevc2|bvc2|av01/i.test(g)) return true;
                const ve = String(br.VideoExtra || '');
                if (/"is_bytevc1"\s*:\s*2|"codec_type"\s*:\s*"(bytevc2|bvc2|av01)/i.test(ve)) return true;
                if (br.play_addr && String(br.play_addr.url_key || '').toLowerCase().includes('bytevc2')) return true;
                return false;
            }

            // 任务 4 (Round 24)：修复 br.VideoExtra 中包含 '"is_bytevc1":0' 导致正则 /bytevc/i 将所有 H.264 源误判为 HEVC/ByteVC1 的严重缺陷
            function isHevcOrByteVCEntry(br) {
                if (!br) return false;
                if (Number(br.is_bytevc1) === 1 || Number(br.is_bytevc1) === 2 || Number(br.is_h265) === 1 || Number(br.is_h265) === 2 || br.is_bytevc1 === true || br.is_h265 === true) return true;
                const g = String(br.gear_name || '') + ' ' + String(br.codec_type || '');
                if (/h265|hevc|bytevc|bvc|av01/i.test(g)) return true;
                const ve = String(br.VideoExtra || '');
                if (/"is_bytevc1"\s*:\s*[12]|"is_h265"\s*:\s*[12]|"codec_type"\s*:\s*"(h265|hevc|bytevc|bvc|av01)/i.test(ve)) return true;
                if (br.play_addr && /bytevc|h265|hevc|bvc/i.test(String(br.play_addr.url_key || ''))) return true;
                return false;
            }

            function appendUrlsFromAddr(addr, outList, maxPerAddr) {
                if (!addr || !Array.isArray(addr.url_list)) return;
                const limit = maxPerAddr || 99;
                let added = 0;
                const sorted = addr.url_list.slice().sort((a, b) => {
                    const score = x => (x.includes('douyinvod.com') ? 3 : (x.includes('zjcdn.com') ? 2 : 0));
                    return score(b) - score(a);
                });
                for (const raw of sorted) {
                    const u = normalizeHttps(raw);
                    if (u && !isInvalidVideoUrl(u) && !u.includes('/aweme/v1/play') && !outList.includes(u)) {
                        outList.push(u);
                        added++;
                        if (added >= limit) break;
                    }
                }
            }

            // 任务 1 (Round 25)：优先选择标准 30fps 渐进式 MP4 档位（normal_720_0 / normal_1080_0 / normal_540_0），排除 DASH 切片轨道（adapt_*）与超高码率 60fps 源，从源头杜绝切片断流与解码器过载
            function pickBestPlayUrl(v) {
                const candidates = [];
                const hasBitRateList = Array.isArray(v.bit_rate) && v.bit_rate.length > 0;
                let allBitRatesAreByteVC2 = hasBitRateList;

                const gearScore = (br) => {
                    if (!br) return -999;
                    const g = String(br.gear_name || '').toLowerCase();
                    if (g.includes('adapt') || br.format === 'dash') return -100;
                    const fps = Number(br.FPS || br.fps || 30);
                    let s = 0;
                    if (fps <= 32) s += 50;
                    if (g.includes('720')) s += 30;
                    else if (g.includes('540')) s += 25;
                    else if (g.includes('1080')) s += 20;
                    return s;
                };

                const sortedBitRates = hasBitRateList ? v.bit_rate.slice().sort((a, b) => {
                    const sa = gearScore(a);
                    const sb = gearScore(b);
                    if (sb !== sa) return sb - sa;
                    const brA = Number((a && a.bit_rate) || 0);
                    const brB = Number((b && b.bit_rate) || 0);
                    return brA - brB;
                }) : [];

                if (hasBitRateList) {
                    for (const br of sortedBitRates) {
                        if (!br) continue;
                        if (!isByteVC2Entry(br)) allBitRatesAreByteVC2 = false;
                        const gName = String(br.gear_name || '').toLowerCase();
                        if (isByteVC2Entry(br) || br.format === 'dash' || gName.includes('adapt')) continue;
                        if (!isHevcOrByteVCEntry(br) && br.play_addr) {
                            appendUrlsFromAddr(br.play_addr, candidates, 2);
                        }
                    }
                }

                if (v.play_addr_264) appendUrlsFromAddr(v.play_addr_264, candidates, 2);
                if (v.play_addr_h264) appendUrlsFromAddr(v.play_addr_h264, candidates, 2);

                if (v.play_addr_lowbr && !isByteVC2Entry(v.play_addr_lowbr) && !isHevcOrByteVCEntry(v.play_addr_lowbr)) {
                    appendUrlsFromAddr(v.play_addr_lowbr, candidates, 2);
                }

                if (!allBitRatesAreByteVC2 && v.download_addr && !isHevcOrByteVCEntry(v.download_addr)) {
                    appendUrlsFromAddr(v.download_addr, candidates, 1);
                }

                if (candidates.length === 0 && !allBitRatesAreByteVC2 && v.play_addr && !isHevcOrByteVCEntry(v.play_addr)) {
                    appendUrlsFromAddr(v.play_addr, candidates, 2);
                }

                return candidates.slice(0, 5).join('@@');
            }

            function isExcludedDouyinUrl(u) {
                if (!u) return false;
                return /favorite|digg|like|collection|collect|history|visited|familiar|following|follow\/feed/i.test(String(u));
            }

            function formatAwemeEntry(aweme) {
                if (!aweme || !aweme.video) return null;
                if (aweme.aweme_type === 68 || aweme.aweme_type === 101 || aweme.aweme_type === 2) return null;
                if (aweme.images && Array.isArray(aweme.images) && aweme.images.length > 0) return null;
                if (aweme.cell_room || aweme.live_reason || aweme.is_ads || aweme.raw_ad_data) return null;
                const v = aweme.video;
                let playUrl = pickBestPlayUrl(v);
                if (!playUrl) return null;
                const rawDurMs = Number(v.duration || aweme.duration || 0);
                if (rawDurMs > 500) {
                    const durSec = (rawDurMs > 1000 ? (rawDurMs / 1000.0) : rawDurMs).toFixed(2);
                    playUrl = playUrl + '@@dur=' + durSec;
                }
                const awemeId = String(aweme.aweme_id || '').trim();
                const secUid = (aweme.author && aweme.author.sec_uid) ? String(aweme.author.sec_uid).trim() : '';
                if (awemeId) playUrl += '@@aweme_id=' + awemeId;
                if (secUid) playUrl += '@@sec_uid=' + secUid;
                const rawDesc = String(aweme.desc || '抖音推荐视频').trim();
                const desc = rawDesc.length > 70 ? rawDesc.substring(0, 70) : rawDesc;
                const author = (aweme.author && aweme.author.nickname) ? ('@' + aweme.author.nickname) : '@抖音创作者';
                let cover = '';
                if (v.cover && Array.isArray(v.cover.url_list) && v.cover.url_list[0]) cover = v.cover.url_list[0];
                else if (v.origin_cover && Array.isArray(v.origin_cover.url_list) && v.origin_cover.url_list[0]) cover = v.origin_cover.url_list[0];
                else if (v.dynamic_cover && Array.isArray(v.dynamic_cover.url_list) && v.dynamic_cover.url_list[0]) cover = v.dynamic_cover.url_list[0];
                cover = normalizeHttps(cover);
                if (!cover) return null;
                const idKey = awemeId || String((v.play_addr && v.play_addr.uri) || playUrl.split('@@')[0] || '').trim();
                return { idKey, desc, author, cover, playUrl };
            }

            function recordAweme(aweme) {
                if (!aweme || !aweme.video) return;
                // 任务 3 (Round 18)：排除用户曾经点过赞 / 收藏过的视频项，防止抖音网页端个人点赞区或收藏区视频混入推荐流造成重复内容
                if (
                    aweme.user_digged === 1 ||
                    aweme.user_digged === true ||
                    aweme.is_digged === 1 ||
                    aweme.is_digged === true ||
                    aweme.collect_stat === 1 ||
                    aweme.user_favorited === 1 ||
                    (aweme.status && (aweme.status.is_digged === 1 || aweme.status.is_digged === true))
                ) return;
                const formatted = formatAwemeEntry(aweme);
                if (!formatted || !formatted.idKey || window.__FC_SEEN__.has(formatted.idKey)) return;
                window.__FC_SEEN__.add(formatted.idKey);
                window.__FC_QUEUE__.push([formatted.desc, formatted.author, formatted.cover, formatted.playUrl]);
            }

            // 任务 6 (Round 23)：在抖音同源 WebView 内执行官方点赞与收藏接口
            window.__FC_DOUYIN_INTERACT__ = async function(awemeId, actionType, active) {
                try {
                    if (!awemeId) return;
                    if (actionType === 'like') {
                        const typeVal = active ? 1 : 0;
                        await fetch('https://www.douyin.com/aweme/v1/web/commit/item/digg/?device_platform=webapp&aid=6383&channel=channel_pc_web&pc_client_type=1', {
                            method: 'POST',
                            credentials: 'include',
                            headers: {
                                'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
                                'Accept': 'application/json, text/plain, */*'
                            },
                            body: 'aweme_id=' + encodeURIComponent(awemeId) + '&item_type=0&type=' + typeVal
                        });
                    } else if (actionType === 'collect') {
                        const actVal = active ? 1 : 0;
                        await fetch('https://www.douyin.com/aweme/v1/web/aweme/collect/?device_platform=webapp&aid=6383&channel=channel_pc_web&pc_client_type=1', {
                            method: 'POST',
                            credentials: 'include',
                            headers: {
                                'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
                                'Accept': 'application/json, text/plain, */*'
                            },
                            body: 'action=' + actVal + '&aweme_id=' + encodeURIComponent(awemeId) + '&aweme_type=0'
                        });
                    }
                } catch (_) {}
            };

            function xhrGetDouyinJson(url) {
                return new Promise(resolve => {
                    try {
                        const xhr = new XMLHttpRequest();
                        xhr.open('GET', url, true);
                        xhr.withCredentials = true;
                        xhr.setRequestHeader('Accept', 'application/json, text/plain, */*');
                        xhr.timeout = 5500;
                        xhr.onload = function() {
                            try {
                                resolve(origJsonParse(xhr.responseText || '{}'));
                            } catch (_) {
                                resolve(null);
                            }
                        };
                        xhr.onerror = function() { resolve(null); };
                        xhr.ontimeout = function() { resolve(null); };
                        xhr.send();
                    } catch (_) {
                        resolve(null);
                    }
                });
            }

            // 任务 7 (Round 23) & 任务 6 (Round 24)：在抖音同源 WebView 内通过 XMLHttpRequest（自动触发 bdms a_bogus 签名）+ 用户主页 SSR 解析 + 关联推荐流多路获取指定创作者的作品列表（只看作者）
            window.__FC_FETCH_DOUYIN_AUTHOR__ = async function(secUid, awemeId, authorName, authorKey) {
                try {
                    const out = [];
                    const seenIds = new Set();
                    const pushEntry = (aw) => {
                        const f = formatAwemeEntry(aw);
                        if (f && f.idKey && !seenIds.has(f.idKey)) {
                            seenIds.add(f.idKey);
                            out.push([f.desc, f.author, f.cover, f.playUrl]);
                        }
                    };

                    let targetSecUid = String(secUid || '').trim();
                    const cleanAuthor = String(authorName || '').replace(/^@+/, '').trim();

                    if (!targetSecUid && awemeId) {
                        const dj = await xhrGetDouyinJson('/aweme/v1/web/aweme/detail/?device_platform=webapp&aid=6383&channel=channel_pc_web&pc_client_type=1&version_code=190500&version_name=19.5.0&aweme_id=' + encodeURIComponent(awemeId));
                        if (dj && dj.aweme_detail && dj.aweme_detail.author) {
                            targetSecUid = String(dj.aweme_detail.author.sec_uid || '').trim();
                        }
                    }

                    if (targetSecUid) {
                        const pj = await xhrGetDouyinJson('/aweme/v1/web/aweme/post/?device_platform=webapp&aid=6383&channel=channel_pc_web&sec_user_id=' + encodeURIComponent(targetSecUid) + '&max_cursor=0&locate_query=false&show_live_replay_strategy=1&need_time_list=1&time_list_query=0&whale_cut_token=&cut_version=1&count=18&publish_video_strategy_type=2&pc_client_type=1&version_code=190500&version_name=19.5.0&cookie_enabled=true&platform=PC');
                        if (pj && Array.isArray(pj.aweme_list)) {
                            for (const aw of pj.aweme_list) pushEntry(aw);
                        }
                    }

                    if (out.length < 2 && targetSecUid) {
                        try {
                            const ur = await fetch('https://www.douyin.com/user/' + encodeURIComponent(targetSecUid), { credentials: 'include' });
                            const html = await ur.text();
                            const collectFromObj = (obj, d) => {
                                if (!obj || typeof obj !== 'object' || d > 10) return;
                                if (obj.video && (obj.video.play_addr || obj.video.bit_rate)) {
                                    pushEntry(obj);
                                    return;
                                }
                                if (Array.isArray(obj)) {
                                    for (let i = 0; i < obj.length; i++) collectFromObj(obj[i], d + 1);
                                } else {
                                    for (const k in obj) {
                                        if (Object.prototype.hasOwnProperty.call(obj, k)) collectFromObj(obj[k], d + 1);
                                    }
                                }
                            };
                            const paceRe = /self\.__pace_f\.push\(\[1,"((?:\\.|[^"\\])*)"\]\)/g;
                            let m;
                            while ((m = paceRe.exec(html)) !== null) {
                                try {
                                    const unescaped = origJsonParse('"' + m[1] + '"');
                                    const idx = unescaped.indexOf(':');
                                    const jsonPart = idx !== -1 ? unescaped.substring(idx + 1) : unescaped;
                                    if (jsonPart.includes('play_addr') || jsonPart.includes('aweme_list')) {
                                        collectFromObj(origJsonParse(jsonPart), 0);
                                    }
                                } catch (_) {}
                            }
                        } catch (_) {}
                    }

                    if (out.length < 2 && awemeId) {
                        const rj = await xhrGetDouyinJson('/aweme/v1/web/aweme/related/?device_platform=webapp&aid=6383&channel=channel_pc_web&aweme_id=' + encodeURIComponent(awemeId) + '&count=20&filterGroupon=1&is_Clean_Mode=0&pc_client_type=1&version_code=190500&version_name=19.5.0');
                        if (rj && Array.isArray(rj.aweme_list)) {
                            for (const aw of rj.aweme_list) {
                                if (!cleanAuthor || (aw && aw.author && String(aw.author.nickname || '').includes(cleanAuthor))) {
                                    pushEntry(aw);
                                }
                            }
                            if (out.length < 2) {
                                for (const aw of rj.aweme_list) pushEntry(aw);
                            }
                        }
                    }

                    if (out.length > 0) {
                        document.title = '__FC_DY_AUTHOR__:' + authorKey + '|||' + encodeURIComponent(JSON.stringify(out)) + '|||' + Date.now();
                    }
                } catch (_) {}
            };

            // 任务 10 (Round 17) & 任务 3 (Round 18)：仅扫描实时网络推荐流返回的数据结构，严禁扫描 DOM 静态预构建卡片、SSR 静态快照以及个人点赞/收藏/历史列表
            function deepScan(obj, depth) {
                if (!obj || typeof obj !== 'object' || depth > 11) return;
                if (obj.video && (obj.video.play_addr || obj.video.bit_rate)) {
                    recordAweme(obj);
                    return;
                }
                if (Array.isArray(obj)) {
                    for (let i = 0; i < obj.length; i++) deepScan(obj[i], depth + 1);
                } else {
                    for (const k in obj) {
                        if (
                            k === '_SSR_DATA' ||
                            k === 'renderData' ||
                            k === 'initialState' ||
                            k === 'favorite_list' ||
                            k === 'digg_list' ||
                            k === 'like_list' ||
                            k === 'collect_list' ||
                            k === 'history_list'
                        ) continue;
                        if (Object.prototype.hasOwnProperty.call(obj, k)) {
                            deepScan(obj[k], depth + 1);
                        }
                    }
                }
            }

            const origJsonParse = JSON.parse;
            const hookedJsonParse = function(text, reviver) {
                const res = origJsonParse.call(this, text, reviver);
                try {
                    if (typeof text === 'string' && text.length < 2400000 && (text.includes('"aweme_list"') || text.includes('"cards"') || text.includes('"aweme_info"') || text.includes('"play_addr"'))) {
                        setTimeout(() => deepScan(res, 0), 0);
                    }
                } catch(e) {}
                return res;
            };
            hookedJsonParse.toString = () => 'function parse() { [native code] }';
            Object.defineProperty(JSON, 'parse', { value: hookedJsonParse, configurable: true, writable: true });

            const origResJson = Response.prototype.json;
            const hookedResJson = function() {
                const respUrl = (this && this.url) ? String(this.url) : '';
                return origResJson.apply(this, arguments).then(data => {
                    try {
                        if (!isExcludedDouyinUrl(respUrl) && !/user\/post|aweme\/post/i.test(respUrl)) {
                            setTimeout(() => deepScan(data, 0), 0);
                        }
                    } catch(e) {}
                    return data;
                });
            };
            hookedResJson.toString = () => 'function json() { [native code] }';
            Object.defineProperty(Response.prototype, 'json', { value: hookedResJson, configurable: true, writable: true });

            try {
                const origXhrOpen = XMLHttpRequest.prototype.open;
                const origXhrSend = XMLHttpRequest.prototype.send;
                XMLHttpRequest.prototype.open = function(method, url) {
                    this.__fc_url = String(url || '');
                    return origXhrOpen.apply(this, arguments);
                };
                XMLHttpRequest.prototype.send = function() {
                    this.addEventListener('load', function() {
                        try {
                            const u = this.__fc_url || '';
                            if (!isExcludedDouyinUrl(u) && !/user\/post|aweme\/post/i.test(u) && /aweme|feed|tab|channel|discover|recommend/i.test(u)) {
                                if (this.responseType === '' || this.responseType === 'text') {
                                    const txt = this.responseText;
                                    if (txt && (txt.includes('play_addr') || txt.includes('aweme_list') || txt.includes('bit_rate') || txt.includes('cards'))) {
                                        deepScan(origJsonParse(txt), 0);
                                    }
                                } else if (this.responseType === 'json' && this.response) {
                                    deepScan(this.response, 0);
                                }
                            }
                        } catch (_) {}
                    });
                    return origXhrSend.apply(this, arguments);
                };
            } catch (_) {}

            // 任务 9 (Round 26)：仅对明确的 JSON API 响应克隆文本，严禁在 URL 对象（reqUrl 为空）或图片/二进制媒体流上调用 resp.clone()，彻底根除 WebKit NetworkProcess HTTP/2 连接池被锁定 60 秒导致抖音全页图片无法显示的问题
            try {
                if (window.fetch) {
                    const origFetch = window.fetch;
                    const hookedFetch = async function(input, init) {
                        const resp = await origFetch.apply(this, arguments);
                        try {
                            const reqUrl = (typeof input === 'string') ? input : (input && input.url ? String(input.url) : (resp && resp.url ? String(resp.url) : ''));
                            const ct = (resp && resp.headers && typeof resp.headers.get === 'function') ? String(resp.headers.get('content-type') || '').toLowerCase() : '';
                            if (
                                reqUrl &&
                                (ct.includes('json') || /\/aweme\/v[12]\/web\//i.test(reqUrl)) &&
                                !/\.(png|jpe?g|webp|gif|avif|svg|mp4|m4s|ts|mp3|woff2?|wasm)(\?|$)/i.test(reqUrl) &&
                                !isExcludedDouyinUrl(reqUrl) &&
                                !/user\/post|aweme\/post/i.test(reqUrl) &&
                                /aweme|feed|tab|channel|discover|recommend/i.test(reqUrl)
                            ) {
                                const cloned = resp.clone();
                                cloned.text().then(txt => {
                                    if (txt && (txt.includes('play_addr') || txt.includes('aweme_list') || txt.includes('bit_rate') || txt.includes('cards'))) {
                                        try { deepScan(origJsonParse(txt), 0); } catch (_) {}
                                    }
                                }).catch(() => {});
                            }
                        } catch (_) {}
                        return resp;
                    };
                    hookedFetch.toString = () => 'function fetch() { [native code] }';
                    window.fetch = hookedFetch;
                }
            } catch (_) {}

            // 任务 1 (Round 17) & 任务 5 (Round 25)：登录窗口打开期间禁止后台模拟滚动；移除全量 DOM 节点遍历并限制滚动步数与频次，防止 CPU 空转
            window.__FC_LOAD_MORE__ = function(forceScroll) {
                if (document.getElementById('fc-login-bar')) return;
                const now = Date.now();
                if (!forceScroll && (now - window.__FC_PAGE_START_TS__ < 2200)) {
                    return;
                }
                if (!forceScroll && (now - (window.__FC_LAST_LOAD_MORE_TS__ || 0) < 2200)) {
                    return;
                }
                window.__FC_LAST_LOAD_MORE_TS__ = now;

                for (let step = 0; step < 2; step++) {
                    setTimeout(() => {
                        try {
                            if (document.getElementById('fc-login-bar')) return;
                            if (window.__pc_pace_load_deferred_scripts) {
                                try { window.__pc_pace_load_deferred_scripts(); } catch(_) {}
                            } else if (window.__pace_load_deferred_scripts) {
                                try { window.__pace_load_deferred_scripts(); } catch(_) {}
                            }
                            const nextBtn = document.querySelector('[data-e2e="video-switch-next-arrow"], .xgplayer-playswitch-next, [class*="playswitch-next"]');
                            if (nextBtn) nextBtn.click();
                            document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', code: 'ArrowDown', keyCode: 40, which: 40, bubbles: true }));
                            window.dispatchEvent(new WheelEvent('wheel', { deltaY: 900, bubbles: true }));
                            window.scrollBy({ top: 900, behavior: 'auto' });
                            const scrollContainer = document.querySelector('#slidelist, [data-e2e="feed-active-video"], .route-scroll-container, main');
                            if (scrollContainer) {
                                if (scrollContainer.scrollTop > 12000) {
                                    scrollContainer.scrollTop = 600;
                                } else {
                                    scrollContainer.scrollTop = scrollContainer.scrollTop + 900;
                                }
                                scrollContainer.dispatchEvent(new Event('scroll', { bubbles: true }));
                            }
                        } catch(e) {}
                    }, step * 280);
                }
            };

            setTimeout(() => { if (window.__FC_SEEN__.size < 6 && window.__FC_LOAD_MORE__) window.__FC_LOAD_MORE__(true); }, 2400);

            window.__FC_SHOW_LOGIN_BAR__ = function() {
                if (document.getElementById('fc-login-bar')) return;
                const bar = document.createElement('div');
                bar.id = 'fc-login-bar';
                bar.style.cssText = 'position:fixed;top:16px;left:50%;transform:translateX(-50%);z-index:2147483647;background:#0A84FF;color:#fff;padding:10px 28px;border-radius:999px;font-family:-apple-system,sans-serif;font-size:15px;font-weight:600;cursor:pointer;box-shadow:0 10px 30px rgba(0,0,0,0.6);border:1.5px solid rgba(255,255,255,0.4);';
                bar.innerText = '✅ 完成登录并返回 Feel Cool TV (或按 Esc / 关闭窗口)';
                bar.onclick = function() {
                    bar.remove();
                    document.title = '__FC_LOGIN_DONE__';
                };
                if (document.body) document.body.appendChild(bar);
            };

            window.addEventListener('contextmenu', function(e) {
                e.preventDefault();
                e.stopPropagation();
                return false;
            }, true);

            window.addEventListener('keydown', function(e) {
                if (e.key === 'Escape' && document.getElementById('fc-login-bar')) {
                    document.getElementById('fc-login-bar').remove();
                    document.title = '__FC_LOGIN_DONE__';
                }
            }, true);

            // 任务 5 (Round 25)：降频静音巡检并移除会导致 xgplayer 无限触发 MediaError 重建 MediaSource 导致 WebKit 进程卡死的 el.removeAttribute('src'); el.load()
            setInterval(() => {
                if (!document.getElementById('fc-login-bar')) {
                    document.querySelectorAll('video, audio').forEach(el => {
                        try {
                            el.muted = true;
                            el.volume = 0;
                            if (!el.paused) el.pause();
                        } catch (_) {}
                    });
                }
                if (Date.now() - (window.__FC_PAGE_START_TS__ || 0) < 12000) {
                    if (window.__pc_pace_load_deferred_scripts) {
                        try { window.__pc_pace_load_deferred_scripts(); } catch(_) {}
                    } else if (window.__pace_load_deferred_scripts) {
                        try { window.__pace_load_deferred_scripts(); } catch(_) {}
                    }
                }
            }, 3000);
        })();
    "#;

    let boot_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1);
    let init_url_str = format!("https://www.douyin.com/discover?_fc_boot={}", boot_ts);
    let parsed_url: tauri::Url = init_url_str.parse().unwrap();
    let modern_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
    let _ = WebviewWindowBuilder::new(app, "douyin_scraper", WebviewUrl::External(parsed_url))
        .title("抖音账号登录")
        .user_agent(modern_ua)
        .inner_size(1180.0, 760.0)
        .visible(false)
        .focused(false)
        .initialization_script(hook_script)
        .on_document_title_changed(move |win, title| {
            if title == "__FC_LOGIN_DONE__" {
                let _ = win.set_always_on_top(false);
                let _ = win.hide();
                feed_arc.lock().unwrap().retain(|k, _| k.starts_with("bili_"));
                probing_arc.lock().unwrap().clear();
                let _ = win.eval("if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.reload();");
                if let Some(main_win) = win.app_handle().get_webview_window("main") {
                    let _ = main_win.set_focus();
                }
            } else if let Some(author_part) = title.strip_prefix("__FC_DY_AUTHOR__:") {
                let parts: Vec<&str> = author_part.split("|||").collect();
                if parts.len() >= 2 {
                    let author_key = parts[0].trim().to_string();
                    let raw_json = url_decode(parts[1]);
                    if let Ok(parsed_items) = serde_json::from_str::<Vec<DouyinItem>>(&raw_json) {
                        if !parsed_items.is_empty() {
                            let feed_clone = feed_arc.clone();
                            let probe_lock_clone = probe_lock_arc.clone();
                            tauri::async_runtime::spawn_blocking(move || {
                                let mut verified_list: Vec<DouyinItem> = Vec::new();
                                for (desc, author, cover, play_url) in parsed_items.into_iter().take(12) {
                                    let aweme_id_opt = extract_stream_meta_tag(&play_url, "aweme_id").map(|s| s.to_string());
                                    let sec_uid_opt = extract_stream_meta_tag(&play_url, "sec_uid").map(|s| s.to_string());
                                    let probed = {
                                        let _g = probe_lock_clone.lock().unwrap();
                                        probe_douyin_candidates(&play_url)
                                    };
                                    if let Some((verified_url, codec, dur_opt)) = probed {
                                        let mut tagged_url = if codec == "h264" {
                                            format!("h264:{}", verified_url)
                                        } else {
                                            verified_url
                                        };
                                        if let Some(d) = dur_opt {
                                            if d > 0.5 {
                                                tagged_url = format!("{}@@dur={:.2}", tagged_url, d);
                                            }
                                        }
                                        if let Some(ref aid) = aweme_id_opt {
                                            if !aid.is_empty() {
                                                tagged_url = format!("{}@@aweme_id={}", tagged_url, aid);
                                            }
                                        }
                                        if let Some(ref suid) = sec_uid_opt {
                                            if !suid.is_empty() {
                                                tagged_url = format!("{}@@sec_uid={}", tagged_url, suid);
                                            }
                                        }
                                        verified_list.push((desc, author, cover, tagged_url));
                                    }
                                }
                                if !verified_list.is_empty() {
                                    let mut fmap = feed_clone.lock().unwrap();
                                    fmap.insert(author_key, verified_list);
                                }
                            });
                        }
                    }
                }
            } else if let Some(payload) = title.strip_prefix("__FC_ITEM__:") {
                let parts: Vec<&str> = payload.split("|||").collect();
                if parts.len() >= 4 {
                    let desc = url_decode(parts[0]);
                    let author = url_decode(parts[1]);
                    let cover = url_decode(parts[2]);
                    let play_url = url_decode(parts[3]);
                    let (clean_candidates, _, _) = strip_stream_meta_suffix(&play_url);
                    let primary_url = clean_candidates.split("@@").next().unwrap_or(clean_candidates).to_string();
                    if !primary_url.starts_with("https://") {
                        return;
                    }
                    {
                        let mut p_set = probing_arc.lock().unwrap();
                        if p_set.contains(&primary_url) {
                            return;
                        }
                        p_set.insert(primary_url.clone());
                    }

                    let active_cat = cat_arc.lock().unwrap().clone();
                    {
                        let fmap = feed_arc.lock().unwrap();
                        if let Some(existing_list) = fmap.get(&active_cat) {
                            if existing_list.len() >= 240 {
                                return;
                            }
                        }
                    }

                    let aweme_id_opt = extract_stream_meta_tag(&play_url, "aweme_id").map(|s| s.to_string());
                    let sec_uid_opt = extract_stream_meta_tag(&play_url, "sec_uid").map(|s| s.to_string());
                    let feed_clone = feed_arc.clone();
                    let probe_lock_clone = probe_lock_arc.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        let probed = {
                            let _probe_guard = probe_lock_clone.lock().unwrap();
                            {
                                let fmap = feed_clone.lock().unwrap();
                                if let Some(existing_list) = fmap.get(&active_cat) {
                                    if existing_list.len() >= 240 {
                                        return;
                                    }
                                }
                            }
                            probe_douyin_candidates(&play_url)
                        };
                        if let Some((verified_url, codec, dur_opt)) = probed {
                            let mut tagged_url = if codec == "h264" {
                                format!("h264:{}", verified_url)
                            } else {
                                verified_url.clone()
                            };
                            if let Some(d) = dur_opt {
                                if d > 0.5 {
                                    tagged_url = format!("{}@@dur={:.2}", tagged_url, d);
                                }
                            }
                            if let Some(ref aid) = aweme_id_opt {
                                if !aid.is_empty() {
                                    tagged_url = format!("{}@@aweme_id={}", tagged_url, aid);
                                }
                            }
                            if let Some(ref suid) = sec_uid_opt {
                                if !suid.is_empty() {
                                    tagged_url = format!("{}@@sec_uid={}", tagged_url, suid);
                                }
                            }
                            let mut feed_map = feed_clone.lock().unwrap();
                            let cat_list = feed_map.entry(active_cat).or_insert_with(Vec::new);
                            if !cat_list.iter().any(|x| x.3.contains(&primary_url)) {
                                cat_list.push((desc, author, cover, tagged_url));
                            }
                        }
                    });
                }
            }
        })
        .build();
}

fn normalize_bili_cover(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("//") {
        format!("https:{}", trimmed)
    } else if trimmed.starts_with("http://") {
        format!("https://{}", &trimmed[7..])
    } else {
        trimmed.to_string()
    }
}

fn parse_bili_item_node(item: &serde_json::Value) -> Option<DouyinItem> {
    let bvid = item
        .get("bvid")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("share_copy").and_then(|v| v.as_str()).filter(|s| s.starts_with("BV")))
        .unwrap_or("")
        .trim();
    if bvid.is_empty() {
        return None;
    }
    let cid = item.get("cid").and_then(|v| v.as_u64()).unwrap_or(0);
    let duration = item.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
    let owner_mid = item
        .get("owner")
        .and_then(|o| o.get("mid"))
        .and_then(|v| v.as_u64())
        .or_else(|| item.get("mid").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let raw_title = item
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("bilibili 视频")
        .replace("<em>", "")
        .replace("</em>", "")
        .trim()
        .to_string();
    let title = if raw_title.chars().count() > 70 {
        raw_title.chars().take(70).collect()
    } else {
        raw_title
    };
    let owner_name = item
        .get("owner")
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| item.get("author").and_then(|v| v.as_str()))
        .unwrap_or("bilibili UP主")
        .trim();
    let author = if owner_name.starts_with('@') {
        owner_name.to_string()
    } else {
        format!("@{}", owner_name)
    };
    let raw_pic = item
        .get("pic")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("cover").and_then(|v| v.as_str()))
        .unwrap_or("");
    let cover = normalize_bili_cover(raw_pic);
    if cover.is_empty() {
        return None;
    }
    let mut play_spec = format!("bili:{}@@{}@@{}", bvid, cid, duration);
    if owner_mid > 0 {
        play_spec = format!("{}@@mid={}", play_spec, owner_mid);
    }
    Some((title, author, cover, play_spec))
}

// 问题 11 (Round 22) & 问题 4/5 (Round 25)：
// 1. 将 Cookie 持久化文件移出 /tmp/cooltv_hls 目录（独立存储在 /tmp/cooltv_bili_cookie_cache.txt），杜绝每次 start_ffmpeg 清空 HLS 切片时误删 bilibili 登录凭证；
// 2. 严禁在 WKWebView KVO 回调（on_document_title_changed）或高频轮询（get_douyin_feed）中调用同步阻塞主线程 CFRunLoop 的 win.cookies_for_url()，仅在显式登录弹窗操作时轻量合并，彻底消除 macOS 主线程重入死锁（旋转彩球无响应）。
const BILI_COOKIE_CACHE_PATH: &str = "/tmp/cooltv_bili_cookie_cache.txt";

fn merge_cookie_strings(base_ck: &str, extra_ck: &str) -> String {
    let mut map: Vec<(String, String)> = Vec::new();
    for src in [base_ck, extra_ck] {
        for part in src.split(';') {
            let kv = part.trim();
            if let Some(eq_idx) = kv.find('=') {
                let k = kv[..eq_idx].trim().to_string();
                let v = kv[eq_idx + 1..].trim().to_string();
                if !k.is_empty() && !v.is_empty() {
                    if let Some(existing) = map.iter_mut().find(|(ek, _)| ek == &k) {
                        existing.1 = v;
                    } else {
                        map.push((k, v));
                    }
                }
            }
        }
    }
    map.into_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

fn persist_bili_cookie(cookie_arc: &Arc<Mutex<String>>, extra_ck: &str) -> String {
    let mut guard = cookie_arc.lock().unwrap();
    let merged = merge_cookie_strings(&guard, extra_ck);
    if !merged.is_empty() && merged != *guard {
        *guard = merged.clone();
        let _ = fs::write(BILI_COOKIE_CACHE_PATH, &merged);
    }
    guard.clone()
}

fn sync_bili_cookies_from_webview(_app: &AppHandle, cookie_arc: &Arc<Mutex<String>>) -> String {
    let cached = cookie_arc.lock().unwrap().clone();
    if !cached.is_empty() {
        return cached;
    }
    if let Ok(disk_ck) = fs::read_to_string(BILI_COOKIE_CACHE_PATH) {
        let trimmed = disk_ck.trim().to_string();
        if !trimmed.is_empty() {
            return persist_bili_cookie(cookie_arc, &trimmed);
        }
    }
    cached
}

fn md5_hex(input: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
        5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
        4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
        0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
        0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
        0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
        0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
        0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
        0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
    ];
    let mut msg = input.to_vec();
    let bit_len = (msg.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    let mut a0: u32 = 0x67452301;
    let mut b0: u32 = 0xefcdab89;
    let mut c0: u32 = 0x98badcfe;
    let mut d0: u32 = 0x10325476;
    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            let idx = i * 4;
            *w = u32::from_le_bytes([chunk[idx], chunk[idx + 1], chunk[idx + 2], chunk[idx + 3]]);
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let temp = d;
            d = c;
            c = b;
            b = b.wrapping_add(a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]).rotate_left(S[i]));
            a = temp;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

fn extract_wbi_mixin_key(img_url: &str, sub_url: &str) -> Option<String> {
    const MIXIN_KEY_ENC_TAB: [usize; 64] = [
        46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49,
        33, 9, 42, 19, 29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40,
        61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11,
        36, 20, 34, 44, 52,
    ];
    let img_key = img_url.rsplit('/').next()?.split('.').next()?;
    let sub_key = sub_url.rsplit('/').next()?.split('.').next()?;
    let raw = format!("{}{}", img_key, sub_key);
    let bytes = raw.as_bytes();
    if bytes.len() < 64 {
        return None;
    }
    let mut mixin = String::with_capacity(32);
    for &idx in MIXIN_KEY_ENC_TAB.iter().take(32) {
        mixin.push(bytes[idx] as char);
    }
    Some(mixin)
}

fn sign_bili_wbi_query(params: &[(&str, String)], mixin_key: &str) -> String {
    let wts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1700000000)
        .to_string();
    let mut sorted: Vec<(&str, String)> = params.to_vec();
    sorted.push(("wts", wts));
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let query = sorted
        .iter()
        .map(|(k, v)| {
            let clean_v: String = v.chars().filter(|c| !"!'()*".contains(*c)).collect();
            format!("{}={}", k, clean_v)
        })
        .collect::<Vec<_>>()
        .join("&");
    let w_rid = md5_hex(format!("{}{}", query, mixin_key).as_bytes());
    format!("{}&w_rid={}", query, w_rid)
}

fn fetch_bilibili_category_items(cat: &str, cookie: &str, page: u32) -> Vec<DouyinItem> {
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
    let curl_json = |api_url: &str| -> Option<serde_json::Value> {
        let mut cmd = Command::new("/usr/bin/curl");
        apply_no_proxy_env(&mut cmd);
        cmd.args(&[
            "-sL",
            "--noproxy",
            "*",
            "--compressed",
            "--connect-timeout",
            "4",
            "--max-time",
            "6",
            "-H",
            &format!("User-Agent: {}", ua),
            "-H",
            "Referer: https://www.bilibili.com/",
            "-H",
            "Origin: https://www.bilibili.com",
        ]);
        if !cookie.trim().is_empty() {
            cmd.args(&["-H", &format!("Cookie: {}", cookie.trim())]);
        }
        cmd.arg(api_url);
        let out = cmd.output().ok()?;
        if !out.status.success() {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
    };

    let mixin_key_opt = curl_json("https://api.bilibili.com/x/web-interface/nav").and_then(|nav_j| {
        let wbi = nav_j.get("data")?.get("wbi_img")?;
        let img_u = wbi.get("img_url")?.as_str()?;
        let sub_u = wbi.get("sub_url")?.as_str()?;
        extract_wbi_mixin_key(img_u, sub_u)
    });

    let mut urls: Vec<String> = Vec::new();
    match cat {
        "bili_rcmd" => {
            if let Some(ref mixin_key) = mixin_key_opt {
                let signed_q = sign_bili_wbi_query(
                    &[
                        ("web_location", "1430650".to_string()),
                        ("y_num", "5".to_string()),
                        ("fresh_type", "4".to_string()),
                        ("feed_version", "V8".to_string()),
                        ("fresh_idx_1h", page.to_string()),
                        ("fetch_row", "1".to_string()),
                        ("fresh_idx", page.to_string()),
                        ("brush", page.to_string()),
                        ("homepage_ver", "1".to_string()),
                        ("ps", "12".to_string()),
                    ],
                    mixin_key,
                );
                urls.push(format!(
                    "https://api.bilibili.com/x/web-interface/wbi/index/top/feed/rcmd?{}",
                    signed_q
                ));
            }
        }
        "bili_popular" => {
            if let Some(ref mixin_key) = mixin_key_opt {
                let signed_q = sign_bili_wbi_query(
                    &[
                        ("ps", "20".to_string()),
                        ("pn", page.to_string()),
                        ("web_location", "333.934".to_string()),
                    ],
                    mixin_key,
                );
                urls.push(format!(
                    "https://api.bilibili.com/x/web-interface/popular?{}",
                    signed_q
                ));
            }
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/popular?ps=20&pn={}",
                page
            ));
            urls.push("https://api.bilibili.com/x/web-interface/ranking/v2?rid=0&type=all".to_string());
        }
        _ => {
            let rid = match cat {
                "bili_anime" => 1,
                "bili_bangumi" => 33,
                "bili_cinephile" => 181,
                "bili_game" => 4,
                "bili_knowledge" => 36,
                "bili_tech" => 188,
                "bili_music" => 3,
                "bili_food" => 211,
                "bili_dance" => 129,
                "bili_sports" => 234,
                _ => 1,
            };
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/dynamic/region?ps=20&pn={}&rid={}",
                page, rid
            ));
            urls.push(format!(
                "https://api.bilibili.com/x/web-interface/ranking/v2?rid={}&type=all",
                rid
            ));
        }
    }

    let mut items: Vec<DouyinItem> = Vec::new();
    for u in &urls {
        if let Some(j) = curl_json(u) {
            if let Some(d) = j.get("data") {
                if let Some(arr) = d
                    .get("item")
                    .and_then(|v| v.as_array())
                    .or_else(|| d.get("archives").and_then(|v| v.as_array()))
                    .or_else(|| d.get("list").and_then(|v| v.as_array()))
                {
                    for node in arr {
                        if let Some(parsed) = parse_bili_item_node(node) {
                            let bvid_prefix = parsed.3.split("@@").next().unwrap_or(&parsed.3);
                            if !items.iter().any(|x| x.3.starts_with(bvid_prefix)) {
                                items.push(parsed);
                            }
                        }
                    }
                }
            }
        }
        if items.len() >= 12 {
            break;
        }
    }
    items
}

fn ensure_bilibili_scraper(
    app: &AppHandle,
    cookie_arc: Arc<Mutex<String>>,
    feed_arc: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>>,
    logged_in_cats_arc: Arc<Mutex<HashSet<String>>>,
) {
    if app.get_webview_window("bilibili_scraper").is_some() {
        return;
    }

    // 任务 2 (Round 18) & 问题 11 (Round 22) & 任务 2, 6, 7 (Round 23) & 任务 4/5 (Round 25)：
    // 1. 修复隐藏 WebView (visible: false) 下 bilibili 首页 <picture><source srcset="..."> 懒加载导致 img.src 为空、DOM 卡片提取 100% 被丢弃的严重缺陷；
    // 2. 增加 window.__pinia / window.__INITIAL_STATE__ 内存 Store 推荐流直接提取与 #__RENDER_DATA__ access_id (w_webid) 签名注入；
    // 3. 引入 __FC_BILI_MSG_QUEUE__ 串行队列并仅在 Cookie 发生变更时同步一次，彻底消除 1500ms 定时器改写 document.title 覆盖 __FC_BILI_FEED__ 以及触发主线程死锁的问题；
    // 4. 携带登录状态标记 (loggedInFlag = 1/0)，当已登录推荐流到达时立即覆盖启动初期的游客缓存，并主动通知主窗口刷新轨道。
    let bili_hook_script = r#"
        (function() {
            try {
                const modernUa = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36';
                Object.defineProperty(navigator, 'userAgent', { get: () => modernUa, configurable: true });
                Object.defineProperty(navigator, 'appVersion', { get: () => '5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36', configurable: true });
                Object.defineProperty(navigator, 'vendor', { get: () => 'Google Inc.', configurable: true });
                Object.defineProperty(window, 'innerWidth', { get: () => 1920, configurable: true });
                Object.defineProperty(window, 'innerHeight', { get: () => 1080, configurable: true });
                Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
                Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
                if (!window.chrome) {
                    window.chrome = { runtime: {} };
                }
                if (window.HTMLMediaElement && window.HTMLMediaElement.prototype) {
                    const origPlay = window.HTMLMediaElement.prototype.play;
                    window.HTMLMediaElement.prototype.play = function() {
                        if (!document.getElementById('fc-login-bar')) {
                            try {
                                this.muted = true;
                                this.pause();
                            } catch (_) {}
                            return Promise.resolve();
                        }
                        return origPlay.apply(this, arguments);
                    };
                }
            } catch (e) {}

            window.__FC_BILI_MSG_QUEUE__ = [];
            window.__FC_LAST_SYNCED_COOKIE__ = '';
            window.__FC_BILI_IS_LOGGED_IN__ = false;

            function enqueueTitleMsg(msg) {
                if (!msg) return;
                window.__FC_BILI_MSG_QUEUE__.push(msg);
            }

            setInterval(() => {
                if (window.__FC_BILI_MSG_QUEUE__.length > 0) {
                    const nextMsg = window.__FC_BILI_MSG_QUEUE__.shift();
                    try {
                        document.title = nextMsg;
                    } catch (_) {}
                }
            }, 65);

            function getCookieVal(name) {
                const m = String(document.cookie || '').match(new RegExp('(?:^|;\\s*)' + name + '=([^;]*)'));
                return m ? decodeURIComponent(m[1]) : '';
            }

            function hasLoginSession() {
                return Boolean(window.__FC_BILI_IS_LOGGED_IN__ || getCookieVal('DedeUserID') || getCookieVal('bili_jct'));
            }

            function checkLowVersionRedirect() {
                try {
                    const href = String(window.location.href || '');
                    const bodyTxt = (document.body && document.body.innerText) ? document.body.innerText : '';
                    if (href.includes('low-version') || href.includes('unsupport') || bodyTxt.includes('浏览器版本过低')) {
                        window.location.replace('https://www.bilibili.com/');
                        return true;
                    }
                } catch (e) {}
                return false;
            }

            checkLowVersionRedirect();

            function md5Js(str) {
                function utf8Bytes(s) {
                    const out = [];
                    for (let i = 0; i < s.length; i++) {
                        let c = s.charCodeAt(i);
                        if (c < 0x80) out.push(c);
                        else if (c < 0x800) { out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f)); }
                        else { out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f)); }
                    }
                    return out;
                }
                const S = [7,12,17,22,7,12,17,22,7,12,17,22,7,12,17,22,5,9,14,20,5,9,14,20,5,9,14,20,5,9,14,20,4,11,16,23,4,11,16,23,4,11,16,23,4,11,16,23,6,10,15,21,6,10,15,21,6,10,15,21,6,10,15,21];
                const K = [0xd76aa478,0xe8c7b756,0x242070db,0xc1bdceee,0xf57c0faf,0x4787c62a,0xa8304613,0xfd469501,0x698098d8,0x8b44f7af,0xffff5bb1,0x895cd7be,0x6b901122,0xfd987193,0xa679438e,0x49b40821,0xf61e2562,0xc040b340,0x265e5a51,0xe9b6c7aa,0xd62f105d,0x02441453,0xd8a1e681,0xe7d3fbc8,0x21e1cde6,0xc33707d6,0xf4d50d87,0x455a14ed,0xa9e3e905,0xfcefa3f8,0x676f02d9,0x8d2a4c8a,0xfffa3942,0x8771f681,0x6d9d6122,0xfde5380c,0xa4beea44,0x4bdecfa9,0xf6bb4b60,0xbebfbc70,0x289b7ec6,0xeaa127fa,0xd4ef3085,0x04881d05,0xd9d4d039,0xe6db99e5,0x1fa27cf8,0xc4ac5665,0xf4292244,0x432aff97,0xab9423a7,0xfc93a039,0x655b59c3,0x8f0ccc92,0xffeff47d,0x85845dd1,0x6fa87e4f,0xfe2ce6e0,0xa3014314,0x4e0811a1,0xf7537e82,0xbd3af235,0x2ad7d2bb,0xeb86d391];
                const msg = utf8Bytes(str);
                const bitLen = msg.length * 8;
                msg.push(0x80);
                while ((msg.length % 64) !== 56) msg.push(0);
                const low = bitLen >>> 0;
                const high = Math.floor(bitLen / 4294967296) >>> 0;
                for (let i = 0; i < 4; i++) msg.push((low >>> (i * 8)) & 0xff);
                for (let i = 0; i < 4; i++) msg.push((high >>> (i * 8)) & 0xff);
                let a0 = 0x67452301, b0 = 0xefcdab89, c0 = 0x98badcfe, d0 = 0x10325476;
                for (let off = 0; off < msg.length; off += 64) {
                    const M = new Uint32Array(16);
                    for (let j = 0; j < 16; j++) {
                        const idx = off + j * 4;
                        M[j] = (msg[idx]) | (msg[idx+1] << 8) | (msg[idx+2] << 16) | (msg[idx+3] << 24);
                    }
                    let A = a0, B = b0, C = c0, D = d0;
                    for (let i = 0; i < 64; i++) {
                        let F = 0, g = 0;
                        if (i < 16) { F = (B & C) | ((~B) & D); g = i; }
                        else if (i < 32) { F = (D & B) | ((~D) & C); g = (5 * i + 1) % 16; }
                        else if (i < 48) { F = B ^ C ^ D; g = (3 * i + 5) % 16; }
                        else { F = C ^ (B | (~D)); g = (7 * i) % 16; }
                        const sum = (A + F + K[i] + M[g]) >>> 0;
                        const rot = ((sum << S[i]) | (sum >>> (32 - S[i]))) >>> 0;
                        const nextB = (B + rot) >>> 0;
                        A = D; D = C; C = B; B = nextB;
                    }
                    a0 = (a0 + A) >>> 0; b0 = (b0 + B) >>> 0; c0 = (c0 + C) >>> 0; d0 = (d0 + D) >>> 0;
                }
                function toHexLE(n) {
                    let s = '';
                    for (let i = 0; i < 4; i++) {
                        const b = (n >>> (i * 8)) & 0xff;
                        s += (b < 16 ? '0' : '') + b.toString(16);
                    }
                    return s;
                }
                return toHexLE(a0) + toHexLE(b0) + toHexLE(c0) + toHexLE(d0);
            }

            let cachedMixinKey = '';
            let cachedMixinTs = 0;
            let cachedWWebId = '';

            function extractWWebIdFromDom() {
                if (cachedWWebId) return cachedWWebId;
                try {
                    const el = document.getElementById('__RENDER_DATA__');
                    if (el && el.textContent) {
                        const decoded = decodeURIComponent(el.textContent.trim());
                        const j = JSON.parse(decoded);
                        if (j && j.access_id) {
                            cachedWWebId = String(j.access_id);
                            return cachedWWebId;
                        }
                    }
                } catch (_) {}
                return '';
            }

            async function getWbiMixinKey() {
                if (cachedMixinKey && (Date.now() - cachedMixinTs < 600000)) return cachedMixinKey;
                try {
                    const r = await fetch('https://api.bilibili.com/x/web-interface/nav', { credentials: 'include' });
                    const j = await r.json();
                    if (j && j.data) {
                        if (j.data.isLogin) {
                            window.__FC_BILI_IS_LOGGED_IN__ = true;
                        }
                        if (j.data.wbi_img) {
                            const imgKey = String(j.data.wbi_img.img_url || '').split('/').pop().split('.')[0] || '';
                            const subKey = String(j.data.wbi_img.sub_url || '').split('/').pop().split('.')[0] || '';
                            const raw = imgKey + subKey;
                            const TAB = [46,47,18,2,53,8,23,32,15,50,10,31,58,3,45,35,27,43,5,49,33,9,42,19,29,28,14,39,12,38,41,13,37,48,7,16,24,55,40,61,26,17,0,1,60,51,30,4,22,25,54,21,56,59,6,63,57,62,11,36,20,34,44,52];
                            if (raw.length >= 64) {
                                cachedMixinKey = TAB.slice(0, 32).map(i => raw[i]).join('');
                                cachedMixinTs = Date.now();
                                return cachedMixinKey;
                            }
                        }
                    }
                } catch (_) {}
                return '';
            }

            async function signWbiParams(paramsObj) {
                const mixin = await getWbiMixinKey();
                const wts = Math.floor(Date.now() / 1000);
                const all = Object.assign({}, paramsObj, { wts: String(wts) });
                const keys = Object.keys(all).sort();
                const q = keys.map(k => {
                    const v = String(all[k]).replace(/[!'()*]/g, '');
                    return encodeURIComponent(k) + '=' + encodeURIComponent(v);
                }).join('&');
                if (!mixin) return q;
                const w_rid = md5Js(q + mixin);
                return q + '&w_rid=' + w_rid;
            }

            function normalizeCoverUrl(rawPic) {
                if (!rawPic || typeof rawPic !== 'string') return '';
                let pic = rawPic.trim();
                const firstPart = pic.split(/\s+/)[0] || '';
                if (firstPart) pic = firstPart;
                if (pic.includes('@')) pic = pic.split('@')[0];
                if (pic.startsWith('//')) pic = 'https:' + pic;
                else if (pic.startsWith('http://')) pic = 'https://' + pic.substring(7);
                return pic.startsWith('https://') ? pic : '';
            }

            function emitBiliNodes(cat, arr) {
                if (!Array.isArray(arr) || !arr.length) return;
                const out = [];
                for (const item of arr) {
                    if (!item || typeof item !== 'object') continue;
                    if (item.goto && item.goto !== 'av' && item.goto !== 'ugs') continue;
                    const bvid = String(item.bvid || '').trim();
                    if (!bvid || !bvid.startsWith('BV')) continue;
                    const cid = Number(item.cid || 0);
                    const dur = Number(item.duration || 0);
                    const rawTitle = String(item.title || 'bilibili 视频').replace(/<[^>]+>/g, '').trim();
                    if (!rawTitle) continue;
                    const ownerName = (item.owner && item.owner.name) ? String(item.owner.name).trim() : String(item.author || 'bilibili UP主').trim();
                    const ownerMid = (item.owner && item.owner.mid) ? Number(item.owner.mid) : Number(item.mid || 0);
                    const author = ownerName.startsWith('@') ? ownerName : ('@' + ownerName);
                    const pic = normalizeCoverUrl(String(item.pic || item.cover || ''));
                    if (!pic) continue;
                    let spec = 'bili:' + bvid + '@@' + cid + '@@' + dur;
                    if (ownerMid > 0) spec += '@@mid=' + ownerMid;
                    out.push([rawTitle.substring(0, 70), author, pic, spec]);
                }
                if (out.length > 0) {
                    const isAuth = hasLoginSession() ? '1' : '0';
                    enqueueTitleMsg('__FC_BILI_FEED__:' + cat + '|||' + isAuth + '|||' + encodeURIComponent(JSON.stringify(out)) + '|||' + Date.now());
                }
            }

            function parseDurationTextToSec(txt) {
                if (!txt) return 0;
                const parts = String(txt).trim().split(':').map(x => parseInt(x, 10));
                if (parts.some(isNaN)) return 0;
                if (parts.length === 3) return parts[0] * 3600 + parts[1] * 60 + parts[2];
                if (parts.length === 2) return parts[0] * 60 + parts[1];
                return 0;
            }

            function scrapeBiliPiniaOrInitialState() {
                try {
                    const candidates = [];
                    if (window.__pinia && window.__pinia.state && window.__pinia.state.value) {
                        const st = window.__pinia.state.value;
                        for (const k in st) {
                            const mod = st[k];
                            if (mod && typeof mod === 'object') {
                                for (const fk of ['recommendList', 'feedList', 'item', 'list', 'rcmdList', 'cards']) {
                                    if (Array.isArray(mod[fk]) && mod[fk].length >= 4) {
                                        candidates.push(...mod[fk]);
                                    }
                                }
                            }
                        }
                    }
                    if (window.__INITIAL_STATE__) {
                        const is = window.__INITIAL_STATE__;
                        for (const fk of ['recommendList', 'feedList', 'item', 'list']) {
                            if (Array.isArray(is[fk]) && is[fk].length >= 4) {
                                candidates.push(...is[fk]);
                            }
                        }
                    }
                    if (candidates.length >= 4) {
                        emitBiliNodes('bili_rcmd', candidates);
                        return true;
                    }
                } catch (_) {}
                return false;
            }

            function extractCardCoverFromDom(card) {
                if (!card) return '';
                const imgEl = card.querySelector('picture img, .bili-video-card__image img, .bili-video-card__cover img, img');
                if (imgEl) {
                    for (const raw of [
                        imgEl.currentSrc,
                        imgEl.getAttribute('src'),
                        imgEl.getAttribute('data-src'),
                        imgEl.getAttribute('data-lazy-src'),
                        imgEl.dataset ? imgEl.dataset.src : ''
                    ]) {
                        const norm = normalizeCoverUrl(raw);
                        if (norm) return norm;
                    }
                }
                const sourceEls = card.querySelectorAll('picture source, source[srcset]');
                for (let i = 0; i < sourceEls.length; i++) {
                    const ss = sourceEls[i].getAttribute('srcset') || sourceEls[i].srcset || '';
                    const norm = normalizeCoverUrl(ss);
                    if (norm) return norm;
                }
                return '';
            }

            function scrapeBiliHomepageDomCards() {
                try {
                    if (!String(window.location.hostname || '').includes('www.bilibili.com')) return false;
                    if (scrapeBiliPiniaOrInitialState()) return true;
                    const cards = document.querySelectorAll('.bili-video-card, .feed-card, .bili-feed-card');
                    const domItems = [];
                    const seenBv = new Set();
                    cards.forEach(card => {
                        if (card.closest('.floor-single-card') || card.querySelector('.bili-live-card') || card.querySelector('.bili-video-card__info--ad')) return;
                        const linkEl = card.querySelector('a[href*="/video/BV"]');
                        if (!linkEl) return;
                        const m = String(linkEl.href || linkEl.getAttribute('href') || '').match(/\/(BV[0-9A-Za-z]+)/);
                        if (!m || !m[1] || seenBv.has(m[1])) return;
                        const bvid = m[1];
                        const titleEl = card.querySelector('.bili-video-card__info--tit, h3.bili-video-card__info--tit a, .bili-video-card__info--tit a');
                        const rawTitle = titleEl ? String(titleEl.getAttribute('title') || titleEl.textContent || '').trim() : '';
                        if (!rawTitle || rawTitle.length < 2) return;
                        const authorEl = card.querySelector('.bili-video-card__info--author, .bili-video-card__info--owner');
                        const authorName = authorEl ? String(authorEl.getAttribute('title') || authorEl.textContent || '').trim() : 'bilibili UP主';
                        const ownerLink = card.querySelector('a.bili-video-card__info--owner[href*="space.bilibili.com/"], a[href*="space.bilibili.com/"]');
                        let mid = 0;
                        if (ownerLink) {
                            const mm = String(ownerLink.href || ownerLink.getAttribute('href') || '').match(/space\.bilibili\.com\/(\d+)/);
                            if (mm && mm[1]) mid = Number(mm[1]);
                        }
                        const pic = extractCardCoverFromDom(card);
                        if (!pic) return;
                        const durEl = card.querySelector('.bili-video-card__stats__duration');
                        const durSec = durEl ? parseDurationTextToSec(durEl.textContent) : 0;
                        seenBv.add(bvid);
                        domItems.push({
                            bvid: bvid,
                            cid: 0,
                            duration: durSec,
                            title: rawTitle,
                            owner: { name: authorName, mid: mid },
                            pic: pic
                        });
                    });
                    if (domItems.length >= 3) {
                        emitBiliNodes('bili_rcmd', domItems);
                        return true;
                    }
                } catch (_) {}
                return false;
            }

            function inspectBiliApiPayload(url, data) {
                try {
                    if (!data || !data.data) return;
                    const d = data.data;
                    const arr = d.item || d.archives || d.list;
                    if (!Array.isArray(arr) || !arr.length) return;
                    const u = String(url || '');
                    if (u.includes('/top/feed/rcmd')) {
                        emitBiliNodes('bili_rcmd', arr);
                    } else if (u.includes('/popular')) {
                        emitBiliNodes('bili_popular', arr);
                    }
                } catch (_) {}
            }

            try {
                if (window.fetch) {
                    const origFetch = window.fetch;
                    window.fetch = async function(input, init) {
                        const resp = await origFetch.apply(this, arguments);
                        try {
                            const reqUrl = (typeof input === 'string') ? input : (input && input.url ? input.url : '');
                            if (reqUrl && (reqUrl.includes('/top/feed/rcmd') || reqUrl.includes('/popular'))) {
                                resp.clone().json().then(j => inspectBiliApiPayload(reqUrl, j)).catch(() => {});
                            }
                        } catch (_) {}
                        return resp;
                    };
                }
            } catch (_) {}

            try {
                const origXhrOpen = XMLHttpRequest.prototype.open;
                const origXhrSend = XMLHttpRequest.prototype.send;
                XMLHttpRequest.prototype.open = function(method, url) {
                    this.__fc_bili_url = String(url || '');
                    return origXhrOpen.apply(this, arguments);
                };
                XMLHttpRequest.prototype.send = function() {
                    this.addEventListener('load', function() {
                        try {
                            const u = this.__fc_bili_url || '';
                            if (u.includes('/top/feed/rcmd') || u.includes('/popular')) {
                                if (this.responseType === '' || this.responseType === 'text') {
                                    inspectBiliApiPayload(u, JSON.parse(this.responseText));
                                } else if (this.responseType === 'json' && this.response) {
                                    inspectBiliApiPayload(u, this.response);
                                }
                            }
                        } catch (_) {}
                    });
                    return origXhrSend.apply(this, arguments);
                };
            } catch (_) {}

            const BILI_CAT_RID_MAP = {
                bili_anime: 1,
                bili_bangumi: 33,
                bili_cinephile: 181,
                bili_game: 4,
                bili_knowledge: 36,
                bili_tech: 188,
                bili_music: 3,
                bili_food: 211,
                bili_dance: 129,
                bili_sports: 234
            };

            window.__FC_FETCH_BILI__ = async function(cat, page) {
                try {
                    const p = Number(page || 1);
                    if (cat === 'bili_rcmd') {
                        if (p === 1 && scrapeBiliHomepageDomCards()) {
                            // 首屏已从同源 DOM/Pinia 拿到卡片，继续在后台拉取一套 WBI 推荐接口进行补充
                        }
                        const rollBtn = document.querySelector('.feed-roll-btn, button.primary-btn.roll-btn, .roll-btn-wrap button');
                        if (rollBtn && p > 1) {
                            try { rollBtn.click(); } catch (_) {}
                        }
                        const rcmdParams = {
                            web_location: '1430650',
                            y_num: '5',
                            fresh_type: '4',
                            feed_version: 'V8',
                            fresh_idx_1h: String(p),
                            fetch_row: '1',
                            fresh_idx: String(p),
                            brush: String(p),
                            homepage_ver: '1',
                            ps: '12'
                        };
                        const wWebid = extractWWebIdFromDom();
                        if (wWebid) rcmdParams.w_webid = wWebid;
                        const signedQ = await signWbiParams(rcmdParams);
                        const r = await fetch('https://api.bilibili.com/x/web-interface/wbi/index/top/feed/rcmd?' + signedQ, { credentials: 'include' });
                        const j = await r.json();
                        if (j && j.data && Array.isArray(j.data.item) && j.data.item.length > 0) {
                            emitBiliNodes('bili_rcmd', j.data.item);
                            return;
                        }
                        scrapeBiliHomepageDomCards();
                        return;
                    }
                    if (cat === 'bili_popular') {
                        const signedPop = await signWbiParams({ ps: '20', pn: String(p), web_location: '333.934' });
                        const r = await fetch('https://api.bilibili.com/x/web-interface/popular?' + signedPop, { credentials: 'include' });
                        const j = await r.json();
                        if (j && j.data && Array.isArray(j.data.list) && j.data.list.length > 0) {
                            emitBiliNodes('bili_popular', j.data.list);
                            return;
                        }
                        return;
                    }
                    const rid = BILI_CAT_RID_MAP[cat];
                    if (rid) {
                        const r = await fetch('https://api.bilibili.com/x/web-interface/dynamic/region?ps=20&pn=' + p + '&rid=' + rid, { credentials: 'include' });
                        const j = await r.json();
                        if (j && j.data && Array.isArray(j.data.archives) && j.data.archives.length > 0) {
                            emitBiliNodes(cat, j.data.archives);
                            return;
                        }
                    }
                } catch (_) {}
            };

            window.__FC_BILI_INTERACT__ = async function(bvid, actionType, active) {
                try {
                    const csrf = getCookieVal('bili_jct');
                    const uid = getCookieVal('DedeUserID');
                    if (!csrf || !bvid) return;
                    if (actionType === 'like') {
                        const body = 'bvid=' + encodeURIComponent(bvid) + '&like=' + (active ? '1' : '2') + '&csrf=' + encodeURIComponent(csrf);
                        await fetch('https://api.bilibili.com/x/web-interface/archive/like', {
                            method: 'POST',
                            credentials: 'include',
                            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                            body: body
                        });
                    } else if (actionType === 'collect') {
                        const vr = await fetch('https://api.bilibili.com/x/web-interface/view?bvid=' + encodeURIComponent(bvid), { credentials: 'include' });
                        const vj = await vr.json();
                        const aid = vj && vj.data ? vj.data.aid : 0;
                        if (!aid || !uid) return;
                        const fr = await fetch('https://api.bilibili.com/x/v3/fav/folder/created/list-all?up_mid=' + encodeURIComponent(uid) + '&type=2', { credentials: 'include' });
                        const fj = await fr.json();
                        const list = fj && fj.data && Array.isArray(fj.data.list) ? fj.data.list : [];
                        if (!list.length) return;
                        const mediaId = list[0].id;
                        const body = 'rid=' + aid + '&type=2&add_media_ids=' + (active ? mediaId : '') + '&del_media_ids=' + (active ? '' : mediaId) + '&csrf=' + encodeURIComponent(csrf) + '&platform=web';
                        await fetch('https://api.bilibili.com/x/v3/fav/resource/deal', {
                            method: 'POST',
                            credentials: 'include',
                            headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
                            body: body
                        });
                    }
                } catch (_) {}
            };

            window.__FC_FETCH_BILI_AUTHOR__ = async function(midStr, bvid, page, feedKey) {
                try {
                    let mid = Number(midStr || 0);
                    let ownerName = '';
                    if ((!mid || !ownerName) && bvid) {
                        const vr = await fetch('https://api.bilibili.com/x/web-interface/view?bvid=' + encodeURIComponent(bvid), { credentials: 'include' });
                        const vj = await vr.json();
                        if (vj && vj.data && vj.data.owner) {
                            if (!mid) mid = Number(vj.data.owner.mid || 0);
                            ownerName = String(vj.data.owner.name || '').trim();
                        }
                    }
                    const p = Number(page || 1);
                    if (mid > 0) {
                        const r = await fetch('https://api.bilibili.com/x/series/recArchivesByKeywords?mid=' + mid + '&keywords=&ps=20&pn=' + p, { credentials: 'include' });
                        const j = await r.json();
                        if (j && j.data && Array.isArray(j.data.archives) && j.data.archives.length > 0) {
                            const mapped = j.data.archives.map(a => ({
                                bvid: a.bvid,
                                cid: 0,
                                duration: a.duration,
                                title: a.title,
                                owner: { name: ownerName || 'bilibili UP主', mid: mid },
                                pic: a.pic
                            }));
                            emitBiliNodes(feedKey, mapped);
                            return;
                        }
                        const wbiQ = await signWbiParams({
                            mid: String(mid),
                            ps: '20',
                            tid: '0',
                            pn: String(p),
                            keyword: '',
                            order: 'pubdate',
                            platform: 'web',
                            web_location: '1550101',
                            order_avoided: 'true'
                        });
                        const sr = await fetch('https://api.bilibili.com/x/space/wbi/arc/search?' + wbiQ, { credentials: 'include' });
                        const sj = await sr.json();
                        if (sj && sj.data && sj.data.list && Array.isArray(sj.data.list.vlist) && sj.data.list.vlist.length > 0) {
                            const mapped = sj.data.list.vlist.map(v => ({
                                bvid: v.bvid,
                                cid: 0,
                                duration: parseDurationTextToSec(v.length) || Number(v.length || 0),
                                title: v.title,
                                owner: { name: v.author || ownerName || 'bilibili UP主', mid: mid },
                                pic: v.pic
                            }));
                            emitBiliNodes(feedKey, mapped);
                            return;
                        }
                    }
                    if (bvid) {
                        const rr = await fetch('https://api.bilibili.com/x/web-interface/archive/related?bvid=' + encodeURIComponent(bvid), { credentials: 'include' });
                        const rj = await rr.json();
                        if (rj && Array.isArray(rj.data) && rj.data.length > 0) {
                            const sameAuthor = rj.data.filter(x => x && x.owner && ((mid && Number(x.owner.mid) === mid) || (ownerName && x.owner.name === ownerName)));
                            const listToUse = sameAuthor.length > 0 ? sameAuthor : rj.data;
                            emitBiliNodes(feedKey, listToUse);
                        }
                    }
                } catch (_) {}
            };

            window.__FC_SYNC_BILI_COOKIE__ = function(forceSync) {
                try {
                    const ck = String(document.cookie || '').trim();
                    if (!ck) return;
                    if (!forceSync && ck === window.__FC_LAST_SYNCED_COOKIE__) return;
                    if (ck.includes('SESSDATA') || ck.includes('bili_jct') || ck.includes('DedeUserID') || ck.includes('buvid3')) {
                        const prevHadUid = window.__FC_LAST_SYNCED_COOKIE__.includes('DedeUserID=');
                        const nowHasUid = ck.includes('DedeUserID=');
                        window.__FC_LAST_SYNCED_COOKIE__ = ck;
                        enqueueTitleMsg('__FC_BILI_COOKIE__:' + encodeURIComponent(ck) + '|||' + Date.now());
                        if (!prevHadUid && nowHasUid) {
                            window.__FC_BILI_IS_LOGGED_IN__ = true;
                            setTimeout(() => {
                                if (window.__FC_FETCH_BILI__) {
                                    window.__FC_FETCH_BILI__('bili_rcmd', 1);
                                }
                            }, 250);
                        }
                    }
                } catch(e) {}
            };

            setTimeout(() => {
                if (window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(true);
                scrapeBiliHomepageDomCards();
                if (window.__FC_FETCH_BILI__) window.__FC_FETCH_BILI__('bili_rcmd', 1);
            }, 1200);

            setTimeout(() => {
                scrapeBiliHomepageDomCards();
            }, 2800);

            // 任务 5 (Round 25)：降频静音巡检并移除会导致播放器无限重建 MediaSource 的 el.removeAttribute('src'); el.load()
            setInterval(() => {
                if (checkLowVersionRedirect()) return;
                if (!document.getElementById('fc-login-bar')) {
                    document.querySelectorAll('video, audio').forEach(el => {
                        try {
                            el.muted = true;
                            el.volume = 0;
                            if (!el.paused) el.pause();
                        } catch (_) {}
                    });
                }
                if (window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(false);
            }, 3500);

            window.__FC_SHOW_LOGIN_BAR__ = function() {
                if (checkLowVersionRedirect()) return;
                if (document.getElementById('fc-login-bar')) return;
                const bar = document.createElement('div');
                bar.id = 'fc-login-bar';
                bar.style.cssText = 'position:fixed;top:16px;left:50%;transform:translateX(-50%);z-index:2147483647;background:#0A84FF;color:#fff;padding:10px 28px;border-radius:999px;font-family:-apple-system,sans-serif;font-size:15px;font-weight:600;cursor:pointer;box-shadow:0 10px 30px rgba(0,0,0,0.6);border:1.5px solid rgba(255,255,255,0.4);';
                bar.innerText = '✅ 完成登录并返回 Feel Cool TV (或按 Esc / 关闭窗口)';
                bar.onclick = function() {
                    bar.remove();
                    try {
                        enqueueTitleMsg('__FC_BILI_LOGIN_DONE__:' + encodeURIComponent(document.cookie || ''));
                    } catch(e) {
                        enqueueTitleMsg('__FC_BILI_LOGIN_DONE__:');
                    }
                };
                if (document.body) document.body.appendChild(bar);
            };

            window.addEventListener('contextmenu', function(e) {
                e.preventDefault();
                e.stopPropagation();
                return false;
            }, true);

            window.addEventListener('keydown', function(e) {
                if (e.key === 'Escape') {
                    e.preventDefault();
                    if (document.getElementById('fc-login-bar')) {
                        document.getElementById('fc-login-bar').remove();
                        try {
                            enqueueTitleMsg('__FC_BILI_LOGIN_DONE__:' + encodeURIComponent(document.cookie || ''));
                        } catch(err) {
                            enqueueTitleMsg('__FC_BILI_LOGIN_DONE__:');
                        }
                    }
                }
            }, true);
        })();
    "#;

    let parsed_url: tauri::Url = "https://www.bilibili.com/".parse().unwrap();
    let modern_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
    let _ = WebviewWindowBuilder::new(app, "bilibili_scraper", WebviewUrl::External(parsed_url))
        .title("bilibili 账号登录")
        .user_agent(modern_ua)
        .inner_size(1180.0, 760.0)
        .visible(false)
        .focused(false)
        .initialization_script(bili_hook_script)
        .on_document_title_changed(move |win, title| {
            if let Some(ck_part) = title.strip_prefix("__FC_BILI_LOGIN_DONE__:") {
                let raw_ck = url_decode(ck_part.split("|||").next().unwrap_or(ck_part));
                if !raw_ck.trim().is_empty() {
                    persist_bili_cookie(&cookie_arc, &raw_ck);
                }
                let _ = win.set_always_on_top(false);
                let _ = win.hide();
                {
                    let mut fmap = feed_arc.lock().unwrap();
                    fmap.retain(|k, _| !k.starts_with("bili_"));
                }
                {
                    let mut lset = logged_in_cats_arc.lock().unwrap();
                    lset.clear();
                }
                let _ = win.eval("if (!String(window.location.hostname || '').includes('www.bilibili.com')) { window.location.href = 'https://www.bilibili.com/'; } else if (window.__FC_FETCH_BILI__) { window.__FC_FETCH_BILI__('bili_rcmd', 1); window.__FC_FETCH_BILI__('bili_popular', 1); }");
                if let Some(main_win) = win.app_handle().get_webview_window("main") {
                    let _ = main_win.set_focus();
                    let _ = main_win.eval("if(window.__FC_ON_BILI_LOGIN_UPDATED__) window.__FC_ON_BILI_LOGIN_UPDATED__();");
                }
            } else if let Some(ck_part) = title.strip_prefix("__FC_BILI_COOKIE__:") {
                let raw_ck = url_decode(ck_part.split("|||").next().unwrap_or(ck_part));
                if !raw_ck.trim().is_empty() {
                    let prev_has_uid = cookie_arc.lock().unwrap().contains("DedeUserID=");
                    let merged = persist_bili_cookie(&cookie_arc, &raw_ck);
                    let now_has_uid = merged.contains("DedeUserID=");
                    if !prev_has_uid && now_has_uid {
                        {
                            let mut fmap = feed_arc.lock().unwrap();
                            fmap.retain(|k, _| !k.starts_with("bili_"));
                        }
                        {
                            let mut lset = logged_in_cats_arc.lock().unwrap();
                            lset.clear();
                        }
                        if let Some(main_win) = win.app_handle().get_webview_window("main") {
                            let _ = main_win.eval("if(window.__FC_ON_BILI_LOGIN_UPDATED__) window.__FC_ON_BILI_LOGIN_UPDATED__();");
                        }
                    }
                }
            } else if let Some(feed_part) = title.strip_prefix("__FC_BILI_FEED__:") {
                let parts: Vec<&str> = feed_part.split("|||").collect();
                if parts.len() >= 2 {
                    let cat_name = parts[0].trim().to_string();
                    let (is_auth, raw_json) = if parts.len() >= 3 && (parts[1] == "0" || parts[1] == "1") {
                        (parts[1] == "1", url_decode(parts[2]))
                    } else {
                        (false, url_decode(parts[1]))
                    };
                    if let Ok(parsed_items) = serde_json::from_str::<Vec<DouyinItem>>(&raw_json) {
                        if !parsed_items.is_empty() {
                            let mut replaced_guest_cache = false;
                            {
                                let mut lset = logged_in_cats_arc.lock().unwrap();
                                if is_auth && !lset.contains(&cat_name) {
                                    lset.insert(cat_name.clone());
                                    replaced_guest_cache = true;
                                }
                            }
                            {
                                let mut fmap = feed_arc.lock().unwrap();
                                let list = fmap.entry(cat_name).or_insert_with(Vec::new);
                                if replaced_guest_cache {
                                    list.clear();
                                }
                                for item in parsed_items {
                                    let bvid_key = item.3.split("@@").next().unwrap_or(&item.3);
                                    if !list.iter().any(|x| x.3.starts_with(bvid_key)) {
                                        list.push(item);
                                    }
                                }
                            }
                            if replaced_guest_cache {
                                if let Some(main_win) = win.app_handle().get_webview_window("main") {
                                    let _ = main_win.eval("if(window.__FC_ON_BILI_LOGIN_UPDATED__) window.__FC_ON_BILI_LOGIN_UPDATED__();");
                                }
                            }
                        }
                    }
                }
            }
        })
        .build();
}

// 任务 7 (Round 11) & 任务 3 (Round 15) & 问题 11 (Round 22) & 任务 2 (Round 23) & 任务 4/5 (Round 25)：
// 优先等待 bilibili_scraper 同源 WebView 返回与网页版登录态完全一致的个性化推荐流，避免 1 秒过早回退游客态 curl
#[tauri::command]
async fn get_douyin_feed(
    app: AppHandle,
    refresh: bool,
    load_more: Option<bool>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DouyinItem>, String> {
    let req_cat = category.unwrap_or_else(|| "discover".to_string());

    if req_cat.starts_with("bili_") {
        ensure_bilibili_scraper(
            &app,
            state.bili_cookie.clone(),
            state.douyin_feed.clone(),
            state.bili_logged_in_cats.clone(),
        );
        let want_more = load_more.unwrap_or(false);
        let feed_arc = state.douyin_feed.clone();
        let page_arc = state.bili_page_map.clone();
        let cookie_str = sync_bili_cookies_from_webview(&app, &state.bili_cookie);
        let cat_key = req_cat.clone();

        if refresh {
            {
                let mut feed_map = feed_arc.lock().unwrap();
                feed_map.remove(&cat_key);
            }
            let next_page = {
                let mut pmap = page_arc.lock().unwrap();
                let p = pmap.entry(cat_key.clone()).or_insert(0);
                *p = (*p % 15) + 1;
                *p
            };
            if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
                let _ = bili_win.eval(&format!(
                    "if(window.__FC_FETCH_BILI__) window.__FC_FETCH_BILI__('{}', {});",
                    cat_key, next_page
                ));
            }
            let feed_arc_wait = state.douyin_feed.clone();
            let req_cat_wait = req_cat.clone();
            let waited_items = tauri::async_runtime::spawn_blocking(move || {
                for _ in 0..38 {
                    std::thread::sleep(Duration::from_millis(65));
                    let wv_items = {
                        let feed_map = feed_arc_wait.lock().unwrap();
                        feed_map.get(&req_cat_wait).cloned().unwrap_or_default()
                    };
                    if !wv_items.is_empty() {
                        return wv_items;
                    }
                }
                Vec::new()
            })
            .await
            .unwrap_or_default();
            if !waited_items.is_empty() {
                return Ok(waited_items);
            }

            let fetched = tauri::async_runtime::spawn_blocking(move || {
                fetch_bilibili_category_items(&cat_key, &cookie_str, next_page)
            })
            .await
            .unwrap_or_default();

            if !fetched.is_empty() {
                let mut feed_map = state.douyin_feed.lock().unwrap();
                let list = feed_map.entry(req_cat.clone()).or_insert_with(Vec::new);
                for item in fetched {
                    let bvid_key = item.3.split("@@").next().unwrap_or(&item.3);
                    if !list.iter().any(|x| x.3.starts_with(bvid_key)) {
                        list.push(item);
                    }
                }
                return Ok(list.clone());
            }
            let after_wv = {
                let feed_map = state.douyin_feed.lock().unwrap();
                feed_map.get(&req_cat).cloned().unwrap_or_default()
            };
            return Ok(after_wv);
        }

        let existing = {
            let feed_map = state.douyin_feed.lock().unwrap();
            feed_map.get(&req_cat).cloned().unwrap_or_default()
        };
        let prev_len = existing.len();

        if existing.is_empty() || want_more {
            let next_page = {
                let mut pmap = page_arc.lock().unwrap();
                let p = pmap.entry(cat_key.clone()).or_insert(0);
                *p = (*p % 18) + 1;
                *p
            };
            if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
                let _ = bili_win.eval(&format!(
                    "if(window.__FC_FETCH_BILI__) window.__FC_FETCH_BILI__('{}', {});",
                    cat_key, next_page
                ));
            }
            let feed_arc_wait = state.douyin_feed.clone();
            let req_cat_wait = req_cat.clone();
            let waited_items = tauri::async_runtime::spawn_blocking(move || {
                for _ in 0..38 {
                    std::thread::sleep(Duration::from_millis(65));
                    let wv_items = {
                        let feed_map = feed_arc_wait.lock().unwrap();
                        feed_map.get(&req_cat_wait).cloned().unwrap_or_default()
                    };
                    if wv_items.len() > prev_len {
                        return Some(wv_items);
                    }
                }
                None
            })
            .await
            .unwrap_or(None);
            if let Some(wv_items) = waited_items {
                return Ok(wv_items);
            }

            let fetched = tauri::async_runtime::spawn_blocking(move || {
                fetch_bilibili_category_items(&cat_key, &cookie_str, next_page)
            })
            .await
            .unwrap_or_default();

            if !fetched.is_empty() {
                let mut feed_map = state.douyin_feed.lock().unwrap();
                let list = feed_map.entry(req_cat.clone()).or_insert_with(Vec::new);
                for item in fetched {
                    let bvid_key = item.3.split("@@").next().unwrap_or(&item.3);
                    if !list.iter().any(|x| x.3.starts_with(bvid_key)) {
                        list.push(item);
                    }
                }
                return Ok(list.clone());
            }
        }

        return Ok(existing);
    }

    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
        state.douyin_probe_lock.clone(),
    );

    if refresh {
        {
            let mut cur_cat = state.douyin_category.lock().unwrap();
            *cur_cat = req_cat.clone();
        }
        {
            let mut feed_map = state.douyin_feed.lock().unwrap();
            feed_map.remove(&req_cat);
        }
        {
            let mut p_set = state.douyin_probing.lock().unwrap();
            p_set.clear();
        }
        let base_url = category_to_douyin_url(&req_cat);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(1);
        let sep = if base_url.contains('?') { "&" } else { "?" };
        let target_url = format!("{}{}_fc_t={}", base_url, sep, ts);
        if let Some(win) = app.get_webview_window("douyin_scraper") {
            let js = format!(
                "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = '{}';",
                target_url
            );
            let _ = win.eval(&js);
        }
        return Ok(Vec::new());
    }

    let existing = {
        let feed_map = state.douyin_feed.lock().unwrap();
        feed_map.get(&req_cat).cloned().unwrap_or_default()
    };

    let want_more = load_more.unwrap_or(false);
    if existing.is_empty() || want_more {
        let mut cat_switched = false;
        {
            let mut cur_cat = state.douyin_category.lock().unwrap();
            if *cur_cat != req_cat {
                *cur_cat = req_cat.clone();
                cat_switched = true;
            }
        }
        if cat_switched {
            let base_url = category_to_douyin_url(&req_cat);
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(1);
            let sep = if base_url.contains('?') { "&" } else { "?" };
            let target_url = format!("{}{}_fc_t={}", base_url, sep, ts);
            if let Some(win) = app.get_webview_window("douyin_scraper") {
                let js = format!(
                    "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = '{}';",
                    target_url
                );
                let _ = win.eval(&js);
            }
        } else if let Some(win) = app.get_webview_window("douyin_scraper") {
            let force_arg = if want_more && !existing.is_empty() { "true" } else { "false" };
            let _ = win.eval(&format!("if(window.__FC_LOAD_MORE__) window.__FC_LOAD_MORE__({});", force_arg));
        }
    }

    Ok(existing)
}

#[tauri::command]
async fn open_douyin_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
        state.douyin_probe_lock.clone(),
    );
    if let Some(win) = app.get_webview_window("douyin_scraper") {
        let _ = win.show();
        let _ = win.center();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.eval("if(window.__FC_SHOW_LOGIN_BAR__) window.__FC_SHOW_LOGIN_BAR__(); try { window.scrollTo(0, 0); document.querySelectorAll('#slidelist, .route-scroll-container, main').forEach(el => { el.scrollTop = 0; }); document.querySelectorAll('img').forEach(img => { img.loading = 'eager'; const ds = img.getAttribute('data-src'); if (!img.src && ds) img.src = ds; }); window.dispatchEvent(new Event('scroll')); window.dispatchEvent(new Event('resize')); } catch(_) {}");
    }
    Ok(())
}

#[tauri::command]
async fn hide_douyin_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("douyin_scraper") {
        let _ = win.set_always_on_top(false);
        let _ = win.hide();
        state.douyin_feed.lock().unwrap().retain(|k, _| k.starts_with("bili_"));
        state.douyin_probing.lock().unwrap().clear();
        let _ = win.eval("const b = document.getElementById('fc-login-bar'); if(b) b.remove(); if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.reload();");
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn open_bilibili_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    ensure_bilibili_scraper(
        &app,
        state.bili_cookie.clone(),
        state.douyin_feed.clone(),
        state.bili_logged_in_cats.clone(),
    );
    if let Some(win) = app.get_webview_window("bilibili_scraper") {
        let _ = win.show();
        let _ = win.center();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.eval("if (String(window.location.href || '').includes('low-version') || (document.body && document.body.innerText && document.body.innerText.includes('浏览器版本过低'))) { window.location.replace('https://passport.bilibili.com/login'); } else if (!String(document.cookie || '').includes('DedeUserID') && !String(window.location.href || '').includes('passport.bilibili.com')) { window.location.href = 'https://passport.bilibili.com/login'; } setTimeout(() => { if(window.__FC_SHOW_LOGIN_BAR__) window.__FC_SHOW_LOGIN_BAR__(); }, 200);");
    }
    Ok(())
}

#[tauri::command]
async fn hide_bilibili_login(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("bilibili_scraper") {
        let _ = win.eval("if(window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(true); const b = document.getElementById('fc-login-bar'); if(b) b.remove(); if (!String(window.location.hostname || '').includes('www.bilibili.com')) { window.location.href = 'https://www.bilibili.com/'; } else if (window.__FC_FETCH_BILI__) { window.__FC_FETCH_BILI__('bili_rcmd', 1); window.__FC_FETCH_BILI__('bili_popular', 1); }");
        let _ = win.set_always_on_top(false);
        let _ = win.hide();
        state.douyin_feed.lock().unwrap().retain(|k, _| !k.starts_with("bili_"));
        state.bili_logged_in_cats.lock().unwrap().clear();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.set_focus();
        let _ = main_win.eval("if(window.__FC_ON_BILI_LOGIN_UPDATED__) window.__FC_ON_BILI_LOGIN_UPDATED__();");
    }
    Ok(())
}

// 任务 12 (Round 5) & 任务 4 (Round 16)：电源管理——彻底清理子进程并安全关闭退出或无白屏热重启应用
#[tauri::command]
async fn quit_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let old_child = state.ffmpeg_process.lock().ok().and_then(|mut g| g.take());
    if let Some((_, mut child)) = old_child {
        let _ = child.kill();
        let _ = child.wait();
    }
    cleanup_orphan_hls_ffmpeg();
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn restart_app(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.ffmpeg_generation.fetch_add(1, Ordering::SeqCst);
    let old_child = state.ffmpeg_process.lock().ok().and_then(|mut g| g.take());
    if let Some((_, mut child)) = old_child {
        let _ = child.kill();
        let _ = child.wait();
    }
    cleanup_orphan_hls_ffmpeg();
    if let Ok(entries) = fs::read_dir("/tmp/cooltv_hls") {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let _ = fs::remove_file(p);
            }
        }
    }
    if let Ok(mut fmap) = state.douyin_feed.lock() {
        fmap.clear();
    }
    if let Ok(mut pset) = state.douyin_probing.lock() {
        pset.clear();
    }
    if let Ok(mut lset) = state.bili_logged_in_cats.lock() {
        lset.clear();
    }
    if let Ok(mut cat) = state.douyin_category.lock() {
        *cat = "discover".to_string();
    }
    if let Some(dy_win) = app.get_webview_window("douyin_scraper") {
        let _ = dy_win.set_always_on_top(false);
        let _ = dy_win.hide();
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(1);
        let _ = dy_win.eval(&format!(
            "if(window.__FC_PURGE_STORAGE__) window.__FC_PURGE_STORAGE__(); window.__FC_PAGE_START_TS__ = Date.now(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.href = 'https://www.douyin.com/discover?_fc_boot={}';",
            ts
        ));
    }
    if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
        let _ = bili_win.set_always_on_top(false);
        let _ = bili_win.hide();
    }
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.eval("window.location.reload();");
        let _ = main_win.set_focus();
    }
    Ok(())
}

// 任务 10 (Round 20)：查询与直接切换 macOS 系统级 VPN 开关（基于 scutil --nc 与 networksetup）
fn list_macos_vpn_services() -> Vec<(String, String, bool)> {
    let mut services: Vec<(String, String, bool)> = Vec::new();
    if let Ok(out) = Command::new("/usr/sbin/scutil").args(&["--nc", "list"]).output() {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('*') || !trimmed.contains("[VPN:") {
                continue;
            }
            let is_connected = trimmed.contains("(Connected)") || trimmed.contains("(Connecting)");
            // 提取 UUID
            let mut uuid_str = String::new();
            for token in trimmed.split_whitespace() {
                if token.len() == 36 && token.chars().filter(|&c| c == '-').count() == 4 {
                    uuid_str = token.to_string();
                    break;
                }
            }
            // 提取引号内的 VPN 服务名称
            let mut name_str = String::new();
            if let Some(q1) = trimmed.find('"') {
                if let Some(q2) = trimmed[q1 + 1..].find('"') {
                    name_str = trimmed[q1 + 1..q1 + 1 + q2].to_string();
                }
            }
            if !name_str.is_empty() || !uuid_str.is_empty() {
                services.push((name_str, uuid_str, is_connected));
            }
        }
    }
    // 优先排序主流代理型 VPN（如 Shadowrocket / Surge / Clash / Stash / Sing-box / WireGuard），其次其他 VPN
    services.sort_by_key(|(name, _, _)| {
        let lower = name.to_lowercase();
        if lower.contains("shadowrocket")
            || lower.contains("surge")
            || lower.contains("clash")
            || lower.contains("stash")
            || lower.contains("sing")
            || lower.contains("quantumult")
            || lower.contains("v2ray")
            || lower.contains("wireguard")
        {
            0
        } else if lower.contains("tailscale") {
            2
        } else {
            1
        }
    });
    services
}

#[tauri::command]
async fn get_system_vpn_status() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let services = list_macos_vpn_services();
        if services.iter().any(|(_, _, connected)| *connected) {
            return Ok(true);
        }
        if let Ok(out) = Command::new("/usr/sbin/scutil").arg("--proxy").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.contains("HTTPEnable : 1")
                || text.contains("HTTPSEnable : 1")
                || text.contains("SOCKSEnable : 1")
            {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .await
    .map_err(|e| format!("查询系统 VPN 状态失败: {}", e))?
}

#[tauri::command]
async fn set_system_vpn_state(enable: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let services = list_macos_vpn_services();
        if enable {
            if let Some((name, uuid, _)) = services.first() {
                if !name.is_empty() {
                    let _ = Command::new("/usr/sbin/scutil")
                        .args(&["--nc", "start", name])
                        .status();
                    let _ = Command::new("/usr/sbin/networksetup")
                        .args(&["-connectpppoeservice", name])
                        .status();
                }
                if !uuid.is_empty() {
                    let _ = Command::new("/usr/sbin/scutil")
                        .args(&["--nc", "start", uuid])
                        .status();
                }
            }
            std::thread::sleep(Duration::from_millis(320));
            let updated = list_macos_vpn_services();
            if updated.iter().any(|(_, _, connected)| *connected) {
                return Ok(true);
            }
            Ok(true)
        } else {
            for (name, uuid, _) in &services {
                if !name.is_empty() {
                    let _ = Command::new("/usr/sbin/scutil")
                        .args(&["--nc", "stop", name])
                        .status();
                    let _ = Command::new("/usr/sbin/networksetup")
                        .args(&["-disconnectpppoeservice", name])
                        .status();
                }
                if !uuid.is_empty() {
                    let _ = Command::new("/usr/sbin/scutil")
                        .args(&["--nc", "stop", uuid])
                        .status();
                }
            }
            std::thread::sleep(Duration::from_millis(250));
            Ok(false)
        }
    })
    .await
    .map_err(|e| format!("切换系统 VPN 状态失败: {}", e))?
}

// 任务 11 (Round 20)：通过 BetterDisplay 命令行工具查询与调节电视亮度（返回值 0.0~1.0 转换为 0~100 纯数字整数）
const BETTER_DISPLAY_BIN: &str = "/Applications/BetterDisplay.app/Contents/MacOS/BetterDisplay";

fn parse_brightness_output_to_percent(raw: &str) -> Option<i32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    for token in trimmed.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-') {
        if token.is_empty() || token == "-" || token == "." {
            continue;
        }
        if let Ok(val) = token.parse::<f64>() {
            if val.is_finite() {
                let pct = if val <= 1.0001 && val >= 0.0 {
                    (val * 100.0).round() as i32
                } else {
                    val.round() as i32
                };
                return Some(pct.clamp(0, 100));
            }
        }
    }
    None
}

fn query_betterdisplay_brightness_percent() -> Option<i32> {
    let out = Command::new(BETTER_DISPLAY_BIN)
        .args(&["get", "-tagID=30", "-brightness"])
        .output()
        .ok()?;
    let stdout_str = String::from_utf8_lossy(&out.stdout);
    parse_brightness_output_to_percent(&stdout_str)
}

#[tauri::command]
async fn get_tv_brightness() -> Result<i32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        query_betterdisplay_brightness_percent()
            .ok_or_else(|| "未能从 BetterDisplay 获取电视亮度".to_string())
    })
    .await
    .map_err(|e| format!("查询亮度异常: {}", e))?
}

#[tauri::command]
async fn adjust_tv_brightness(delta_pct: Option<i32>, delta_percent: Option<i32>) -> Result<i32, String> {
    // 任务 1 (Round 23)：亮度控制步长从 20 改为 10
    let delta = delta_pct.or(delta_percent).unwrap_or(10);
    tauri::async_runtime::spawn_blocking(move || {
        let offset_arg = if delta >= 0 {
            "-brightness=10%"
        } else {
            "-brightness=-10%"
        };
        let _ = Command::new(BETTER_DISPLAY_BIN)
            .args(&[
                "set",
                "-type=Display",
                "-tagID=30",
                offset_arg,
                "-offset",
            ])
            .status();
        Ok(delta)
    })
    .await
    .map_err(|e| format!("调节亮度异常: {}", e))?
}

fn extract_cookie_field(cookie_str: &str, key: &str) -> String {
    for part in cookie_str.split(';') {
        let kv = part.trim();
        if let Some(eq_idx) = kv.find('=') {
            let k = kv[..eq_idx].trim();
            let v = kv[eq_idx + 1..].trim();
            if k == key {
                return v.to_string();
            }
        }
    }
    String::new()
}

fn parse_stream_url_metadata(raw_url: &str) -> (String, String, String) {
    let trimmed = raw_url.trim().trim_start_matches("douyin:");
    let mut plat = "douyin".to_string();
    let mut item_id = String::new();
    let mut author_id = String::new();

    if trimmed.starts_with("bili:") || trimmed.starts_with("bilibili:") || trimmed.contains("bilibili.com") {
        plat = "bilibili".to_string();
        let clean = trimmed.trim_start_matches("bilibili:").trim_start_matches("bili:");
        let parts: Vec<&str> = clean.split("@@").collect();
        if let Some(first) = parts.first() {
            if first.starts_with("BV") {
                item_id = first.trim().to_string();
            }
        }
        for part in parts.iter().skip(1) {
            if let Some(m) = part.strip_prefix("mid=") {
                author_id = m.trim().to_string();
            }
        }
    } else {
        for part in trimmed.split("@@") {
            if let Some(aid) = part.strip_prefix("aweme_id=") {
                item_id = aid.trim().to_string();
            } else if let Some(suid) = part.strip_prefix("sec_uid=") {
                author_id = suid.trim().to_string();
            }
        }
    }
    (plat, item_id, author_id)
}

// 任务 6 (Round 23)：触发流媒体网站官方的点赞 / 收藏功能（支持抖音与 bilibili）
#[tauri::command]
async fn interact_stream_item(
    app: AppHandle,
    url: Option<String>,
    action: Option<String>,
    enable: Option<bool>,
    platform: Option<String>,
    item_id: Option<String>,
    action_type: Option<String>,
    active: Option<bool>,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let (parsed_plat, parsed_item_id, _) = parse_stream_url_metadata(url.as_deref().unwrap_or(""));
    let plat = platform
        .unwrap_or(parsed_plat)
        .trim()
        .to_lowercase();
    let act = action
        .or(action_type)
        .unwrap_or_else(|| "like".to_string())
        .trim()
        .to_lowercase();
    let id_clean = item_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(parsed_item_id)
        .trim()
        .to_string();
    let is_active = enable.or(active).unwrap_or(true);

    if plat == "bilibili" || id_clean.starts_with("BV") {
        ensure_bilibili_scraper(
            &app,
            state.bili_cookie.clone(),
            state.douyin_feed.clone(),
            state.bili_logged_in_cats.clone(),
        );
        let cookie_str = state.bili_cookie.lock().unwrap().clone();
        if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
            let js = format!(
                "if(window.__FC_BILI_INTERACT__) window.__FC_BILI_INTERACT__('{}', '{}', {});",
                id_clean.replace('\'', ""),
                act.replace('\'', ""),
                if is_active { "true" } else { "false" }
            );
            let _ = bili_win.eval(&js);
        }
        let bvid = id_clean.clone();
        let act_clone = act.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let csrf = extract_cookie_field(&cookie_str, "bili_jct");
            let dede_uid = extract_cookie_field(&cookie_str, "DedeUserID");
            if csrf.is_empty() || !bvid.starts_with("BV") {
                return;
            }
            let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
            if act_clone == "like" {
                let like_val = if is_active { "1" } else { "2" };
                let body = format!("bvid={}&like={}&csrf={}", bvid, like_val, csrf);
                let mut cmd = Command::new("/usr/bin/curl");
                apply_no_proxy_env(&mut cmd);
                let _ = cmd
                    .args(&[
                        "-sL",
                        "--noproxy",
                        "*",
                        "-X",
                        "POST",
                        "-H",
                        &format!("User-Agent: {}", ua),
                        "-H",
                        "Referer: https://www.bilibili.com/",
                        "-H",
                        "Origin: https://www.bilibili.com",
                        "-H",
                        "Content-Type: application/x-www-form-urlencoded",
                        "-H",
                        &format!("Cookie: {}", cookie_str),
                        "--data",
                        &body,
                        "https://api.bilibili.com/x/web-interface/archive/like",
                    ])
                    .output();
            } else if act_clone == "collect" {
                let view_url = format!("https://api.bilibili.com/x/web-interface/view?bvid={}", bvid);
                let mut aid: u64 = 0;
                let mut v_cmd = Command::new("/usr/bin/curl");
                apply_no_proxy_env(&mut v_cmd);
                if let Ok(out) = v_cmd
                    .args(&[
                        "-sL",
                        "--noproxy",
                        "*",
                        "-H",
                        &format!("User-Agent: {}", ua),
                        "-H",
                        "Referer: https://www.bilibili.com/",
                        "-H",
                        &format!("Cookie: {}", cookie_str),
                        &view_url,
                    ])
                    .output()
                {
                    if let Ok(vj) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                        aid = vj
                            .get("data")
                            .and_then(|d| d.get("aid"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                    }
                }
                let mut media_id: u64 = 0;
                if !dede_uid.is_empty() {
                    let fav_list_url = format!(
                        "https://api.bilibili.com/x/v3/fav/folder/created/list-all?up_mid={}&type=2",
                        dede_uid
                    );
                    let mut f_cmd = Command::new("/usr/bin/curl");
                    apply_no_proxy_env(&mut f_cmd);
                    if let Ok(out) = f_cmd
                        .args(&[
                            "-sL",
                            "--noproxy",
                            "*",
                            "-H",
                            &format!("User-Agent: {}", ua),
                            "-H",
                            "Referer: https://www.bilibili.com/",
                            "-H",
                            &format!("Cookie: {}", cookie_str),
                            &fav_list_url,
                        ])
                        .output()
                    {
                        if let Ok(fj) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                            media_id = fj
                                .get("data")
                                .and_then(|d| d.get("list"))
                                .and_then(|l| l.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|item| item.get("id"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                        }
                    }
                }
                if aid > 0 && media_id > 0 {
                    let body = if is_active {
                        format!(
                            "rid={}&type=2&add_media_ids={}&del_media_ids=&csrf={}&platform=web",
                            aid, media_id, csrf
                        )
                    } else {
                        format!(
                            "rid={}&type=2&add_media_ids=&del_media_ids={}&csrf={}&platform=web",
                            aid, media_id, csrf
                        )
                    };
                    let mut c_cmd = Command::new("/usr/bin/curl");
                    apply_no_proxy_env(&mut c_cmd);
                    let _ = c_cmd
                        .args(&[
                            "-sL",
                            "--noproxy",
                            "*",
                            "-X",
                            "POST",
                            "-H",
                            &format!("User-Agent: {}", ua),
                            "-H",
                            "Referer: https://www.bilibili.com/",
                            "-H",
                            "Origin: https://www.bilibili.com",
                            "-H",
                            "Content-Type: application/x-www-form-urlencoded",
                            "-H",
                            &format!("Cookie: {}", cookie_str),
                            "--data",
                            &body,
                            "https://api.bilibili.com/x/v3/fav/resource/deal",
                        ])
                        .output();
                }
            }
        })
        .await;
        return Ok(is_active);
    }

    // 抖音官方点赞与收藏
    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
        state.douyin_probe_lock.clone(),
    );
    if let Some(dy_win) = app.get_webview_window("douyin_scraper") {
        let js = format!(
            "if(window.__FC_DOUYIN_INTERACT__) window.__FC_DOUYIN_INTERACT__('{}', '{}', {});",
            id_clean.replace('\'', ""),
            act.replace('\'', ""),
            if is_active { "true" } else { "false" }
        );
        let _ = dy_win.eval(&js);
    }
    Ok(is_active)
}

// 任务 7 (Round 23)：获取指定流媒体作者（UP主 / 抖音创作者）的视频列表，用于“只看作者”模式上下键切视频
#[tauri::command]
async fn get_stream_author_videos(
    app: AppHandle,
    url: Option<String>,
    author: Option<String>,
    platform: Option<String>,
    author_id: Option<String>,
    item_id: Option<String>,
    author_name: Option<String>,
    page: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Vec<DouyinItem>, String> {
    let (parsed_plat, parsed_item_id, parsed_author_id) = parse_stream_url_metadata(url.as_deref().unwrap_or(""));
    let plat = platform
        .unwrap_or(parsed_plat)
        .trim()
        .to_lowercase();
    let p = page.unwrap_or(1).max(1);
    let resolved_item_id = item_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(parsed_item_id);
    let resolved_author_id = author_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(parsed_author_id);
    let resolved_author_name = author
        .or(author_name)
        .unwrap_or_default();

    if plat == "bilibili" || resolved_item_id.trim().starts_with("BV") {
        ensure_bilibili_scraper(
            &app,
            state.bili_cookie.clone(),
            state.douyin_feed.clone(),
            state.bili_logged_in_cats.clone(),
        );
        let cookie_str = state.bili_cookie.lock().unwrap().clone();
        let bvid = resolved_item_id.trim().to_string();
        let raw_mid = resolved_author_id.trim().to_string();
        let author_label = resolved_author_name.trim().to_string();

        let feed_key = format!("bili_author_{}_{}", if !raw_mid.is_empty() { &raw_mid } else { &bvid }, p);
        if let Some(bili_win) = app.get_webview_window("bilibili_scraper") {
            let js = format!(
                "if(window.__FC_FETCH_BILI_AUTHOR__) window.__FC_FETCH_BILI_AUTHOR__('{}', '{}', {}, '{}');",
                raw_mid.replace('\'', ""),
                bvid.replace('\'', ""),
                p,
                feed_key.replace('\'', "")
            );
            let _ = bili_win.eval(&js);
        }

        let feed_arc_wait = state.douyin_feed.clone();
        let feed_key_wait = feed_key.clone();
        let waited_found = tauri::async_runtime::spawn_blocking(move || {
            for _ in 0..14 {
                std::thread::sleep(Duration::from_millis(65));
                let found = {
                    let fmap = feed_arc_wait.lock().unwrap();
                    fmap.get(&feed_key_wait).cloned().unwrap_or_default()
                };
                if !found.is_empty() {
                    return found;
                }
            }
            Vec::new()
        })
        .await
        .unwrap_or_default();
        if !waited_found.is_empty() {
            return Ok(waited_found);
        }

        let fetched = tauri::async_runtime::spawn_blocking(move || {
            let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";
            let curl_json = |api_url: &str| -> Option<serde_json::Value> {
                let mut cmd = Command::new("/usr/bin/curl");
                apply_no_proxy_env(&mut cmd);
                cmd.args(&[
                    "-sL",
                    "--noproxy",
                    "*",
                    "--compressed",
                    "--connect-timeout",
                    "4",
                    "--max-time",
                    "6",
                    "-H",
                    &format!("User-Agent: {}", ua),
                    "-H",
                    "Referer: https://www.bilibili.com/",
                ]);
                if !cookie_str.trim().is_empty() {
                    cmd.args(&["-H", &format!("Cookie: {}", cookie_str.trim())]);
                }
                cmd.arg(api_url);
                let out = cmd.output().ok()?;
                serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()
            };

            let mut mid_val: u64 = raw_mid.parse::<u64>().unwrap_or(0);
            let mut resolved_owner_name = author_label.clone();
            if (mid_val == 0 || resolved_owner_name.is_empty()) && bvid.starts_with("BV") {
                let view_u = format!("https://api.bilibili.com/x/web-interface/view?bvid={}", bvid);
                if let Some(vj) = curl_json(&view_u) {
                    if let Some(d) = vj.get("data") {
                        if let Some(ow) = d.get("owner") {
                            if mid_val == 0 {
                                mid_val = ow.get("mid").and_then(|v| v.as_u64()).unwrap_or(0);
                            }
                            if resolved_owner_name.is_empty() {
                                if let Some(n) = ow.get("name").and_then(|v| v.as_str()) {
                                    resolved_owner_name = format!("@{}", n.trim().trim_start_matches('@'));
                                }
                            }
                        }
                    }
                }
            }
            let mut out_items: Vec<DouyinItem> = Vec::new();
            if mid_val > 0 {
                let rec_url = format!(
                    "https://api.bilibili.com/x/series/recArchivesByKeywords?mid={}&keywords=&ps=20&pn={}",
                    mid_val, p
                );
                if let Some(rj) = curl_json(&rec_url) {
                    if let Some(arr) = rj
                        .get("data")
                        .and_then(|d| d.get("archives"))
                        .and_then(|v| v.as_array())
                    {
                        for node in arr {
                            let bv = node.get("bvid").and_then(|v| v.as_str()).unwrap_or("").trim();
                            if bv.is_empty() {
                                continue;
                            }
                            let dur = node.get("duration").and_then(|v| v.as_u64()).unwrap_or(0);
                            let raw_t = node.get("title").and_then(|v| v.as_str()).unwrap_or("bilibili 视频").trim();
                            let pic = normalize_bili_cover(node.get("pic").and_then(|v| v.as_str()).unwrap_or(""));
                            if pic.is_empty() {
                                continue;
                            }
                            let disp_author = if resolved_owner_name.is_empty() {
                                "@bilibili UP主".to_string()
                            } else if resolved_owner_name.starts_with('@') {
                                resolved_owner_name.clone()
                            } else {
                                format!("@{}", resolved_owner_name)
                            };
                            let spec = format!("bili:{}@@0@@{}@@mid={}", bv, dur, mid_val);
                            out_items.push((raw_t.chars().take(70).collect(), disp_author, pic, spec));
                        }
                    }
                }
            }
            out_items
        })
        .await
        .unwrap_or_default();
        return Ok(fetched);
    }

    // 抖音“只看作者”
    ensure_douyin_scraper(
        &app,
        state.douyin_feed.clone(),
        state.douyin_probing.clone(),
        state.douyin_category.clone(),
        state.douyin_probe_lock.clone(),
    );
    let sec_uid = resolved_author_id.trim().to_string();
    let aweme_id = resolved_item_id.trim().to_string();
    let author_key = format!("dy_author_{}", if !sec_uid.is_empty() { &sec_uid } else { &resolved_author_name });
    if let Some(dy_win) = app.get_webview_window("douyin_scraper") {
        let js = format!(
            "if(window.__FC_FETCH_DOUYIN_AUTHOR__) window.__FC_FETCH_DOUYIN_AUTHOR__('{}', '{}', '{}', '{}');",
            sec_uid.replace('\'', ""),
            aweme_id.replace('\'', ""),
            resolved_author_name.trim().replace('\'', ""),
            author_key.replace('\'', "")
        );
        let _ = dy_win.eval(&js);
    }
    let feed_arc_dy = state.douyin_feed.clone();
    let author_key_dy = author_key.clone();
    let waited_dy = tauri::async_runtime::spawn_blocking(move || {
        for _ in 0..32 {
            std::thread::sleep(Duration::from_millis(85));
            let found = {
                let fmap = feed_arc_dy.lock().unwrap();
                fmap.get(&author_key_dy).cloned().unwrap_or_default()
            };
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    })
    .await
    .unwrap_or_default();
    Ok(waited_dy)
}

#[tauri::command]
async fn is_window_fullscreen(app: AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.is_fullscreen().ok())
        .unwrap_or(false)
}

#[tauri::command]
async fn set_window_fullscreen_state(app: AppHandle, fullscreen: bool) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_fullscreen(fullscreen);
    }
    Ok(())
}

// 任务 11 (Round 26)：长期本地持久化缓存目录（独立于 /tmp，重启系统永不丢失；支持 TMDB Logo、封面图、IPTV 台标、用户头像及影视评分/介绍元数据长期缓存）
fn get_persistent_cache_dir() -> std::path::PathBuf {
    let base = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/Users/gaobo"));
    let dir = base
        .join("Library")
        .join("Application Support")
        .join("com.feelcool.tv")
        .join("cache");
    let _ = fs::create_dir_all(dir.join("images"));
    let _ = fs::create_dir_all(dir.join("meta"));
    dir
}

#[tauri::command]
async fn cache_remote_image(url: String, force: Option<bool>, force_refresh: Option<bool>) -> Result<String, String> {
    let clean_url = url.trim().to_string();
    if !clean_url.starts_with("http://") && !clean_url.starts_with("https://") {
        return Err("仅支持 HTTP/HTTPS 远程图片".into());
    }
    let force = force.or(force_refresh).unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        let img_dir = get_persistent_cache_dir().join("images");
        let _ = fs::create_dir_all(&img_dir);

        let mut hasher = DefaultHasher::new();
        clean_url.hash(&mut hasher);
        let hash_id = hasher.finish();

        let lower = clean_url.split('?').next().unwrap_or(&clean_url).to_lowercase();
        let ext = if lower.ends_with(".png") {
            "png"
        } else if lower.ends_with(".webp") {
            "webp"
        } else if lower.ends_with(".svg") {
            "svg"
        } else if lower.ends_with(".gif") {
            "gif"
        } else {
            "jpg"
        };
        let filename = format!("img_{}.{}", hash_id, ext);
        let full_path = img_dir.join(&filename);
        let tmp_path = img_dir.join(format!(".tmp_{}", filename));

        let is_valid_img_file = |p: &Path| -> bool {
            if let Ok(meta) = fs::metadata(p) {
                if meta.len() > 64 {
                    if let Ok(bytes) = fs::read(p) {
                        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(128)]).to_lowercase();
                        if !head.contains("<!doctype html") && !head.contains("<html") && !head.contains("404 not found") {
                            return true;
                        }
                    }
                }
            }
            false
        };

        if !force && is_valid_img_file(&full_path) {
            return Ok(format!("http://127.0.0.1:8765/cache_img/{}", filename));
        }

        let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
        let out_str = tmp_path.to_string_lossy().to_string();

        // 先尝试通过系统默认网络（含代理）下载，失败则回退直连
        let _ = Command::new("/usr/bin/curl")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .args(&[
                "-sL",
                "--fail",
                "--max-time",
                "12",
                "-H",
                &format!("User-Agent: {}", ua),
                "-o",
                &out_str,
                &clean_url,
            ])
            .status();

        if !is_valid_img_file(&tmp_path) {
            let mut direct_cmd = Command::new("/usr/bin/curl");
            apply_no_proxy_env(&mut direct_cmd);
            let _ = direct_cmd
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .args(&[
                    "-sL",
                    "--noproxy",
                    "*",
                    "--fail",
                    "--max-time",
                    "12",
                    "-H",
                    &format!("User-Agent: {}", ua),
                    "-o",
                    &out_str,
                    &clean_url,
                ])
                .status();
        }

        if is_valid_img_file(&tmp_path) {
            let _ = fs::rename(&tmp_path, &full_path);
            return Ok(format!("http://127.0.0.1:8765/cache_img/{}", filename));
        }
        let _ = fs::remove_file(&tmp_path);
        if is_valid_img_file(&full_path) {
            return Ok(format!("http://127.0.0.1:8765/cache_img/{}", filename));
        }
        Err("下载缓存图片失败".into())
    })
    .await
    .map_err(|e| format!("缓存异常: {}", e))?
}

#[tauri::command]
async fn load_persistent_metadata() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let meta_file = get_persistent_cache_dir().join("meta").join("media_meta_cache.json");
        if let Ok(content) = fs::read_to_string(&meta_file) {
            if !content.trim().is_empty() {
                return Ok(content);
            }
        }
        Ok("{}".to_string())
    })
    .await
    .map_err(|e| format!("读取元数据缓存失败: {}", e))?
}

#[tauri::command]
async fn save_persistent_metadata(json_data: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let meta_dir = get_persistent_cache_dir().join("meta");
        let _ = fs::create_dir_all(&meta_dir);
        let meta_file = meta_dir.join("media_meta_cache.json");
        let _ = fs::write(&meta_file, json_data.as_bytes());
        Ok(())
    })
    .await
    .map_err(|e| format!("写入元数据缓存失败: {}", e))?
}

#[tauri::command]
async fn clear_local_cache() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cache_dir = get_persistent_cache_dir();
        let img_dir = cache_dir.join("images");
        let meta_dir = cache_dir.join("meta");
        if let Ok(entries) = fs::read_dir(&img_dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        if let Ok(entries) = fs::read_dir(&meta_dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        if let Ok(entries) = fs::read_dir("/tmp/cooltv_hls/thumbs") {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok("已清除本地缓存".to_string())
    })
    .await
    .map_err(|e| format!("清除缓存失败: {}", e))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    cleanup_orphan_hls_ffmpeg();

    let shared_douyin_feed: Arc<Mutex<HashMap<String, Vec<DouyinItem>>>> = Arc::new(Mutex::new(HashMap::new()));
    let shared_douyin_probing: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let shared_douyin_cat: Arc<Mutex<String>> = Arc::new(Mutex::new("discover".to_string()));
    let shared_douyin_probe_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    let shared_bili_logged_in_cats: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    let initial_bili_cookie = fs::read_to_string(BILI_COOKIE_CACHE_PATH)
        .or_else(|_| fs::read_to_string("/tmp/cooltv_hls/bili_cookie_cache.txt"))
        .unwrap_or_default()
        .trim()
        .to_string();
    if !initial_bili_cookie.is_empty() {
        let _ = fs::write(BILI_COOKIE_CACHE_PATH, &initial_bili_cookie);
    }
    let shared_bili_cookie: Arc<Mutex<String>> = Arc::new(Mutex::new(initial_bili_cookie));
    let setup_feed = shared_douyin_feed.clone();
    let setup_probing = shared_douyin_probing.clone();
    let setup_cat = shared_douyin_cat.clone();
    let setup_probe_lock = shared_douyin_probe_lock.clone();
    let setup_bili_cookie = shared_bili_cookie.clone();
    let setup_bili_logged_in = shared_bili_logged_in_cats.clone();

    std::thread::spawn(move || {
        let _ = fs::create_dir_all("/tmp/cooltv_hls/thumbs");
        let persistent_img_dir = get_persistent_cache_dir().join("images");
        let _ = fs::create_dir_all(&persistent_img_dir);
        rouille::start_server("127.0.0.1:8765", move |request| {
            let req_url = request.url();
            if let Some(rel_name) = req_url.strip_prefix("/cache_img/") {
                let clean_name = rel_name.split('?').next().unwrap_or(rel_name);
                if !clean_name.is_empty() && !clean_name.contains("..") && !clean_name.contains('/') {
                    let file_path = persistent_img_dir.join(clean_name);
                    if let Ok(file) = fs::File::open(&file_path) {
                        let mime = if clean_name.ends_with(".png") {
                            "image/png"
                        } else if clean_name.ends_with(".webp") {
                            "image/webp"
                        } else if clean_name.ends_with(".svg") {
                            "image/svg+xml"
                        } else if clean_name.ends_with(".gif") {
                            "image/gif"
                        } else {
                            "image/jpeg"
                        };
                        return rouille::Response::from_file(mime, file)
                            .with_additional_header("Cache-Control", "public, max-age=31536000")
                            .with_additional_header("Access-Control-Allow-Origin", "*");
                    }
                }
            }
            let mut response = rouille::match_assets(&request, "/tmp/cooltv_hls");
            if response.is_success() {
                response.headers.retain(|(k, _)| k.as_ref().to_lowercase() != "cache-control");
                response.headers.push(("Cache-Control".into(), "no-cache, no-store, must-revalidate".into()));
                response.headers.push(("Pragma".into(), "no-cache".into()));
                response.headers.push(("Expires".into(), "0".into()));
                response.headers.push(("Access-Control-Allow-Origin".into(), "*".into()));
                return response;
            }
            rouille::Response::text("Not Found")
                .with_status_code(404)
                .with_additional_header("Access-Control-Allow-Origin", "*")
        });
    });

    tauri::Builder::default()
        .manage(AppState {
            ffmpeg_process: Arc::new(Mutex::new(None)),
            ffmpeg_op_lock: Arc::new(Mutex::new(())),
            ffmpeg_generation: Arc::new(AtomicU64::new(0)),
            douyin_feed: shared_douyin_feed,
            douyin_probing: shared_douyin_probing,
            douyin_category: shared_douyin_cat,
            douyin_probe_lock: shared_douyin_probe_lock,
            bili_cookie: shared_bili_cookie,
            bili_page_map: Arc::new(Mutex::new(HashMap::new())),
            bili_logged_in_cats: shared_bili_logged_in_cats,
            thumb_lock: Arc::new(Mutex::new(())),
            minutecast_400_until: Arc::new(AtomicU64::new(0)),
            was_precipitating: Arc::new(Mutex::new(false)),
        })
        .setup(move |app| {
            // 任务 4 (Round 32)：确保 macOS 顶部应用菜单栏内容全面汉化
            #[cfg(target_os = "macos")]
            {
                use tauri::menu::{Menu, Submenu, PredefinedMenuItem, AboutMetadata};
                let handle = app.handle();
                let about_meta = AboutMetadata {
                    name: Some("Feel Cool TV".to_string()),
                    version: Some("4.4.0".to_string()),
                    ..Default::default()
                };
                if let (
                    Ok(about_item),
                    Ok(services_item),
                    Ok(hide_item),
                    Ok(hide_others_item),
                    Ok(show_all_item),
                    Ok(quit_item),
                    Ok(close_win_item),
                    Ok(undo_item),
                    Ok(redo_item),
                    Ok(cut_item),
                    Ok(copy_item),
                    Ok(paste_item),
                    Ok(select_all_item),
                    Ok(fullscreen_item),
                    Ok(minimize_item),
                    Ok(maximize_item),
                    Ok(sep1),
                    Ok(sep2),
                    Ok(sep3),
                    Ok(sep4),
                ) = (
                    PredefinedMenuItem::about(handle, Some("关于 Feel Cool TV"), Some(about_meta)),
                    PredefinedMenuItem::services(handle, Some("服务")),
                    PredefinedMenuItem::hide(handle, Some("隐藏 Feel Cool TV")),
                    PredefinedMenuItem::hide_others(handle, Some("隐藏其他")),
                    PredefinedMenuItem::show_all(handle, Some("全部显示")),
                    PredefinedMenuItem::quit(handle, Some("退出 Feel Cool TV")),
                    PredefinedMenuItem::close_window(handle, Some("关闭窗口")),
                    PredefinedMenuItem::undo(handle, Some("撤销")),
                    PredefinedMenuItem::redo(handle, Some("重做")),
                    PredefinedMenuItem::cut(handle, Some("剪切")),
                    PredefinedMenuItem::copy(handle, Some("拷贝")),
                    PredefinedMenuItem::paste(handle, Some("粘贴")),
                    PredefinedMenuItem::select_all(handle, Some("全选")),
                    PredefinedMenuItem::fullscreen(handle, Some("进入全屏")),
                    PredefinedMenuItem::minimize(handle, Some("最小化")),
                    PredefinedMenuItem::maximize(handle, Some("缩放")),
                    PredefinedMenuItem::separator(handle),
                    PredefinedMenuItem::separator(handle),
                    PredefinedMenuItem::separator(handle),
                    PredefinedMenuItem::separator(handle),
                ) {
                    if let (
                        Ok(app_menu),
                        Ok(file_menu),
                        Ok(edit_menu),
                        Ok(view_menu),
                        Ok(window_menu),
                    ) = (
                        Submenu::with_items(handle, "Feel Cool TV", true, &[&about_item, &sep1, &services_item, &sep2, &hide_item, &hide_others_item, &show_all_item, &sep3, &quit_item]),
                        Submenu::with_items(handle, "文件", true, &[&close_win_item]),
                        Submenu::with_items(handle, "编辑", true, &[&undo_item, &redo_item, &sep4, &cut_item, &copy_item, &paste_item, &select_all_item]),
                        Submenu::with_items(handle, "显示", true, &[&fullscreen_item]),
                        Submenu::with_items(handle, "窗口", true, &[&minimize_item, &maximize_item]),
                    ) {
                        if let Ok(menu) = Menu::with_items(handle, &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu]) {
                            let _ = app.set_menu(menu);
                        }
                    }
                }
            }
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_title("");
                let _ = main_win.set_title_bar_style(TitleBarStyle::Overlay);
            }
            ensure_douyin_scraper(
                app.handle(),
                setup_feed.clone(),
                setup_probing.clone(),
                setup_cat.clone(),
                setup_probe_lock.clone(),
            );
            ensure_bilibili_scraper(
                app.handle(),
                setup_bili_cookie.clone(),
                setup_feed.clone(),
                setup_bili_logged_in.clone(),
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed = event {
                    let old_child = window
                        .app_handle()
                        .try_state::<AppState>()
                        .and_then(|state| state.ffmpeg_process.lock().ok().and_then(|mut g| g.take()));
                    if let Some((_, mut child)) = old_child {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    cleanup_orphan_hls_ffmpeg();
                    window.app_handle().exit(0);
                }
            } else if window.label() == "douyin_scraper" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.set_always_on_top(false);
                    let _ = window.hide();
                    if let Some(wv) = window.app_handle().get_webview_window("douyin_scraper") {
                        let _ = wv.eval("const b = document.getElementById('fc-login-bar'); if(b) b.remove(); window.__FC_QUEUE__ = []; window.__FC_SEEN__.clear(); window.location.reload();");
                    }
                    if let Some(main_win) = window.app_handle().get_webview_window("main") {
                        let _ = main_win.set_focus();
                    }
                }
            } else if window.label() == "bilibili_scraper" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.set_always_on_top(false);
                    let _ = window.hide();
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        if let Ok(mut fmap) = state.douyin_feed.lock() {
                            fmap.retain(|k, _| !k.starts_with("bili_"));
                        }
                        if let Ok(mut lset) = state.bili_logged_in_cats.lock() {
                            lset.clear();
                        }
                    }
                    if let Some(wv) = window.app_handle().get_webview_window("bilibili_scraper") {
                        let _ = wv.eval("if(window.__FC_SYNC_BILI_COOKIE__) window.__FC_SYNC_BILI_COOKIE__(true); const b = document.getElementById('fc-login-bar'); if(b) b.remove(); if (!String(window.location.hostname || '').includes('www.bilibili.com')) { window.location.href = 'https://www.bilibili.com/'; } else if (window.__FC_FETCH_BILI__) { window.__FC_FETCH_BILI__('bili_rcmd', 1); window.__FC_FETCH_BILI__('bili_popular', 1); }");
                    }
                    if let Some(main_win) = window.app_handle().get_webview_window("main") {
                        let _ = main_win.set_focus();
                        let _ = main_win.eval("if(window.__FC_ON_BILI_LOGIN_UPDATED__) window.__FC_ON_BILI_LOGIN_UPDATED__();");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start_drag,
            start_ffmpeg,
            get_media_duration,
            scan_local_folder,
            extract_video_thumb,
            fetch_douban_rating,
            fetch_epg_data,
            fetch_weather_broadcast,
            get_douyin_feed,
            open_douyin_login,
            hide_douyin_login,
            open_bilibili_login,
            hide_bilibili_login,
            interact_stream_item,
            get_stream_author_videos,
            get_system_vpn_status,
            set_system_vpn_state,
            get_tv_brightness,
            adjust_tv_brightness,
            is_window_fullscreen,
            set_window_fullscreen_state,
            cache_remote_image,
            load_persistent_metadata,
            save_persistent_metadata,
            clear_local_cache,
            quit_app,
            restart_app
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
