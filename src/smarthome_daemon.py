#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
很酷TV (CoolTV) v6.3.0 — 米家智能家居环境传感器、系统信息 & 遥控音量桥接守护服务 (Round 41 ~ Round 43)
1. 室内环境（开始页）：
   - 室内温度：`mi -o plain get blt.3.16e9cjkf4ek00 2.1` -> 附加 `℃`
   - 室内湿度：`mi -o plain get blt.3.16e9cjkf4ek00 2.2` -> 附加 `%`
   - 室内空气质量：`mi -o plain get xiaomi.airp.rma4 3.3` -> 0 代表 `空气良好`，映射为 `空气良好` / `轻度污染` / `中度污染` / `重度污染`
2. 系统信息（设置 -> 通用设置 -> 系统信息）：
   - 系统名称：`很酷自在OS 6`
   - 系统版本号：`6.3.0.1020`
   - macOS 版本、本机 IP 地址、本机 MAC 地址、设备实时内存、CPU 占用
   - 恢复出厂设置接口（仅清理本应用缓存数据，绝不改动 macOS 系统任何数据和设置）
3. 遥控器音量键桥接（/api/volume_step）：
   - 针对蓝牙遥控器守护进程投递 key_code=103 (F11) / key_code=111 (F12) 时在全屏窗口下的系统音量步进控制
"""

import ctypes
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse, parse_qs

APP_VERSION = "6.3.0"
PRECISE_SYSTEM_VERSION = f"{APP_VERSION}.1020"
SYSTEM_OS_NAME = f"很酷自在OS {APP_VERSION.split('.')[0]}"

CACHE_LOCK = threading.Lock()
CACHE_DATA = {
    "indoor_temp": "--℃",
    "indoor_humidity": "--%",
    "indoor_air_quality": "空气良好",
    "updated_at": 0,
}

SYS_INFO_LOCK = threading.Lock()
SYS_INFO_CACHE = {
    "system_name": SYSTEM_OS_NAME,
    "system_version": PRECISE_SYSTEM_VERSION,
    "macos_version": "macOS",
    "ip_address": "127.0.0.1",
    "mac_address": "--",
    "memory_usage": "-- GB / -- GB",
    "cpu_usage": "--%",
    "updated_at": 0,
}

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
CANDIDATE_HOME_JSON_PATHS = [
    p for p in [
        "/tmp/cooltv_hls/home_env.json",
        os.path.join(SCRIPT_DIR, "home_env.json"),
        os.path.join(SCRIPT_DIR, "src", "home_env.json"),
        "/Users/gaobo/Documents/feelcoolMAX/cooltv-mac/src/home_env.json",
    ]
    if ".app/Contents/" not in p
]
CANDIDATE_SYS_JSON_PATHS = [
    p for p in [
        "/tmp/cooltv_hls/system_info.json",
        os.path.join(SCRIPT_DIR, "system_info.json"),
        os.path.join(SCRIPT_DIR, "src", "system_info.json"),
        "/Users/gaobo/Documents/feelcoolMAX/cooltv-mac/src/system_info.json",
    ]
    if ".app/Contents/" not in p
]

# Mach CPU ticks state for instant delta calculation
_PREV_CPU_TICKS = None


def step_macos_system_volume(direction: str) -> dict:
    """调整 macOS 系统输出音量（支持 up / down / mute）"""
    d = (direction or "").strip().lower()
    try:
        if d == "mute":
            script = "set curMuted to output muted of (get volume settings)\nset volume output muted (not curMuted)"
        elif d == "down":
            script = (
                "set v to output volume of (get volume settings)\n"
                "set nextV to v - 6\n"
                "if nextV < 0 then set nextV to 0\n"
                "set volume output muted false output volume nextV"
            )
        else:
            script = (
                "set v to output volume of (get volume settings)\n"
                "set nextV to v + 6\n"
                "if nextV > 100 then set nextV to 100\n"
                "set volume output muted false output volume nextV"
            )
        subprocess.Popen(
            ["/usr/bin/osascript", "-e", script],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return {"ok": True, "dir": d}
    except Exception as e:
        return {"ok": False, "error": str(e)}


def read_mach_cpu_ticks():
    try:
        libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
        host = libc.mach_host_self()
        HOST_CPU_LOAD_INFO = 3
        ticks = (ctypes.c_uint32 * 4)()
        count = ctypes.c_uint32(4)
        kr = libc.host_statistics(host, HOST_CPU_LOAD_INFO, ctypes.byref(ticks), ctypes.byref(count))
        if kr == 0:
            # user, system, idle, nice
            return [int(ticks[0]), int(ticks[1]), int(ticks[2]), int(ticks[3])]
    except Exception:
        pass
    return None


def sample_cpu_usage_percent() -> str:
    global _PREV_CPU_TICKS
    cur = read_mach_cpu_ticks()
    if cur is not None:
        if _PREV_CPU_TICKS is None:
            _PREV_CPU_TICKS = cur
            time.sleep(0.12)
            cur = read_mach_cpu_ticks() or cur
        prev = _PREV_CPU_TICKS
        _PREV_CPU_TICKS = cur
        d_user = max(0, cur[0] - prev[0])
        d_sys = max(0, cur[1] - prev[1])
        d_idle = max(0, cur[2] - prev[2])
        d_nice = max(0, cur[3] - prev[3])
        total = d_user + d_sys + d_idle + d_nice
        if total > 0:
            busy_pct = ((d_user + d_sys + d_nice) / float(total)) * 100.0
            return f"{max(0.1, min(100.0, busy_pct)):.1f}%"
    try:
        io_out = subprocess.run(
            ["/usr/sbin/iostat", "-n", "0", "-c", "2", "-w", "1"],
            capture_output=True,
            text=True,
            timeout=2.5,
        ).stdout
        lines = [l.split() for l in io_out.strip().splitlines() if l.strip()]
        if lines and len(lines[-1]) >= 3:
            idl = float(lines[-1][2])
            return f"{max(0.1, min(100.0, 100.0 - idl)):.1f}%"
    except Exception:
        pass
    return "--%"


def query_static_os_and_net():
    macos_str = "macOS"
    try:
        v = subprocess.run(["/usr/bin/sw_vers", "-productVersion"], capture_output=True, text=True, timeout=2.0).stdout.strip()
        b = subprocess.run(["/usr/bin/sw_vers", "-buildVersion"], capture_output=True, text=True, timeout=2.0).stdout.strip()
        if v and b:
            macos_str = f"macOS {v} ({b})"
        elif v:
            macos_str = f"macOS {v}"
    except Exception:
        pass

    ip_addr = ""
    mac_addr = ""
    try:
        r = subprocess.run(["/sbin/ifconfig"], capture_output=True, text=True, timeout=2.5)
        blocks = re.split(r"(?m)^([a-z0-9]+):\s", r.stdout)
        for i in range(1, len(blocks), 2):
            iface = blocks[i]
            body = blocks[i + 1]
            if iface.startswith(("lo", "utun", "awdl", "llw", "bridge", "gif", "stf", "anpi")):
                continue
            m_ip = re.search(r"inet\s+(\d+\.\d+\.\d+\.\d+)", body)
            m_mac = re.search(r"ether\s+([0-9a-fA-F:]{17})", body)
            if not mac_addr and m_mac and iface == "en0":
                mac_addr = m_mac.group(1).upper()
            if m_ip and not m_ip.group(1).startswith(("127.", "169.254.")):
                ip_addr = m_ip.group(1)
                if m_mac:
                    mac_addr = m_mac.group(1).upper()
                break
    except Exception:
        pass

    return macos_str, (ip_addr or "127.0.0.1"), (mac_addr or "未知")


def query_memory_usage() -> str:
    try:
        tot_b = int(
            subprocess.run(["/usr/sbin/sysctl", "-n", "hw.memsize"], capture_output=True, text=True, timeout=2.0).stdout.strip()
            or "0"
        )
        tot_gb = tot_b / (1024 ** 3)
        vm = subprocess.run(["/usr/bin/vm_stat"], capture_output=True, text=True, timeout=2.0).stdout
        ps_m = re.search(r"page size of (\d+) bytes", vm)
        page_size = int(ps_m.group(1)) if ps_m else 16384

        def pages(k):
            m = re.search(rf"{k}:\s+(\d+)", vm)
            return int(m.group(1)) if m else 0

        used_pages = pages("Pages active") + pages("Pages wired down") + pages("Pages occupied by compressor")
        used_gb = (used_pages * page_size) / (1024 ** 3)
        pct_mem = int(round((used_gb / tot_gb) * 100)) if tot_gb > 0 else 0
        return f"{used_gb:.1f} GB / {tot_gb:.0f} GB ({pct_mem}%)"
    except Exception:
        return "-- GB / -- GB"


def poll_system_info_once() -> dict:
    macos_str, ip_addr, mac_addr = query_static_os_and_net()
    mem_str = query_memory_usage()
    cpu_str = sample_cpu_usage_percent()
    with SYS_INFO_LOCK:
        SYS_INFO_CACHE["system_name"] = SYSTEM_OS_NAME
        SYS_INFO_CACHE["system_version"] = PRECISE_SYSTEM_VERSION
        SYS_INFO_CACHE["macos_version"] = macos_str
        SYS_INFO_CACHE["ip_address"] = ip_addr
        SYS_INFO_CACHE["mac_address"] = mac_addr
        SYS_INFO_CACHE["memory_usage"] = mem_str
        SYS_INFO_CACHE["cpu_usage"] = cpu_str
        SYS_INFO_CACHE["updated_at"] = int(time.time())
        snap = dict(SYS_INFO_CACHE)

    payload_str = json.dumps(snap, ensure_ascii=False)
    for out_path in CANDIDATE_SYS_JSON_PATHS:
        try:
            parent = os.path.dirname(out_path)
            if parent == "/tmp/cooltv_hls":
                os.makedirs(parent, exist_ok=True)
            if os.path.isdir(parent):
                tmp_path = out_path + ".tmp"
                with open(tmp_path, "w", encoding="utf-8") as f:
                    f.write(payload_str)
                os.replace(tmp_path, out_path)
        except Exception:
            pass
    return snap


def build_env():
    env = os.environ.copy()
    if os.path.isdir("/Users/gaobo"):
        env["HOME"] = "/Users/gaobo"
    extra_paths = [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/Users/gaobo/.local/bin",
        "/Users/gaobo/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ]
    cur_path = env.get("PATH", "")
    env["PATH"] = ":".join(extra_paths + ([cur_path] if cur_path else []))
    return env


def run_mi_get(device_id: str, prop_id: str, timeout_sec: float = 8.0) -> str:
    try:
        proc = subprocess.run(
            ["mi", "-o", "plain", "get", device_id, prop_id],
            capture_output=True,
            text=True,
            timeout=timeout_sec,
            env=build_env(),
        )
        if proc.returncode == 0 and proc.stdout:
            lines = [ln.strip() for ln in proc.stdout.strip().splitlines() if ln.strip()]
            if lines:
                tokens = lines[-1].split()
                return tokens[-1].strip() if tokens else lines[-1]
    except Exception:
        pass
    return ""


def format_temp(raw: str) -> str:
    s = (raw or "").strip().replace("℃", "").replace("°C", "").strip()
    if not s:
        return ""
    try:
        v = float(s)
        if abs(v - round(v)) < 0.05:
            return f"{int(round(v))}℃"
        return f"{v:.1f}℃"
    except Exception:
        return ""


def format_humidity(raw: str) -> str:
    s = (raw or "").strip().replace("%", "").strip()
    if not s:
        return ""
    try:
        v = float(s)
        if abs(v - round(v)) < 0.05:
            return f"{int(round(v))}%"
        return f"{v:.1f}%"
    except Exception:
        return ""


def format_air_quality(raw: str) -> str:
    s = (raw or "").strip()
    if not s:
        return "空气良好"
    for label in ("空气良好", "轻度污染", "中度污染", "重度污染"):
        if label in s:
            return label
    if s in ("优", "良"):
        return "空气良好"
    try:
        v = float(s)
        if v == 0 or v <= 1:
            return "空气良好"
        if v == 2:
            return "轻度污染"
        if v == 3:
            return "中度污染"
        if v == 4:
            return "重度污染"
        if v <= 35:
            return "空气良好"
        if v <= 75:
            return "轻度污染"
        if v <= 115:
            return "中度污染"
        return "重度污染"
    except Exception:
        return "空气良好"


def poll_once() -> dict:
    with ThreadPoolExecutor(max_workers=3) as pool:
        f_temp = pool.submit(run_mi_get, "blt.3.16e9cjkf4ek00", "2.1")
        f_hum = pool.submit(run_mi_get, "blt.3.16e9cjkf4ek00", "2.2")
        f_air = pool.submit(run_mi_get, "xiaomi.airp.rma4", "3.3")
        raw_t = f_temp.result()
        raw_h = f_hum.result()
        raw_a = f_air.result()

    t_str = format_temp(raw_t)
    h_str = format_humidity(raw_h)
    a_str = format_air_quality(raw_a)

    with CACHE_LOCK:
        if t_str:
            CACHE_DATA["indoor_temp"] = t_str
        if h_str:
            CACHE_DATA["indoor_humidity"] = h_str
        if a_str:
            CACHE_DATA["indoor_air_quality"] = a_str
        CACHE_DATA["updated_at"] = int(time.time())
        snapshot = dict(CACHE_DATA)

    payload_str = json.dumps(snapshot, ensure_ascii=False)
    for out_path in CANDIDATE_HOME_JSON_PATHS:
        try:
            parent = os.path.dirname(out_path)
            if parent == "/tmp/cooltv_hls":
                os.makedirs(parent, exist_ok=True)
            if os.path.isdir(parent):
                tmp_path = out_path + ".tmp"
                with open(tmp_path, "w", encoding="utf-8") as f:
                    f.write(payload_str)
                os.replace(tmp_path, out_path)
        except Exception:
            pass

    return snapshot


def perform_app_factory_reset() -> dict:
    """仅清理很酷TV应用自身的缓存与持久化元数据文件，绝不改动 macOS 系统任何数据和设置"""
    home_dir = "/Users/gaobo" if os.path.isdir("/Users/gaobo") else os.path.expanduser("~")
    app_support_dirs = [
        os.path.join(home_dir, "Library", "Application Support", "com.gaobo.cooltv-mac"),
        os.path.join(home_dir, "Library", "WebKit", "com.gaobo.cooltv-mac"),
        os.path.join(home_dir, "Library", "Caches", "com.gaobo.cooltv-mac"),
        "/tmp/cooltv_hls",
    ]
    for d in app_support_dirs:
        if os.path.isdir(d):
            try:
                for name in os.listdir(d):
                    if name in ("home_env.json", "system_info.json"):
                        continue
                    fp = os.path.join(d, name)
                    if os.path.isdir(fp):
                        shutil.rmtree(fp, ignore_errors=True)
                    else:
                        try:
                            os.remove(fp)
                        except Exception:
                            pass
            except Exception:
                pass
    return {"ok": True}


def background_loop(parent_pid: int):
    tick = 0
    while True:
        if parent_pid > 0:
            try:
                os.kill(parent_pid, 0)
            except OSError:
                os._exit(0)
        try:
            poll_system_info_once()
        except Exception:
            pass
        if tick % 9 == 0:
            try:
                poll_once()
            except Exception:
                pass
        tick += 1
        for _ in range(5):
            if parent_pid > 0:
                try:
                    os.kill(parent_pid, 0)
                except OSError:
                    os._exit(0)
            time.sleep(1.0)


class HomeEnvHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        return

    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "*")
        self.end_headers()

    def _send_json(self, data: dict):
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cache-Control", "no-store, no-cache, must-revalidate, max-age=0")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        parsed = urlparse(self.path)
        qs = parse_qs(parsed.query)
        if parsed.path == "/api/factory_reset":
            res = perform_app_factory_reset()
            self._send_json(res)
            return
        if parsed.path == "/api/volume_step":
            d = (qs.get("dir", ["up"])[0] or "up").strip()
            res = step_macos_system_volume(d)
            self._send_json(res)
            return
        self._send_json({"ok": True})

    def do_GET(self):
        parsed = urlparse(self.path)
        qs = parse_qs(parsed.query)
        if parsed.path == "/api/factory_reset":
            res = perform_app_factory_reset()
            self._send_json(res)
            return
        if parsed.path == "/api/volume_step":
            d = (qs.get("dir", ["up"])[0] or "up").strip()
            res = step_macos_system_volume(d)
            self._send_json(res)
            return
        if parsed.path == "/api/system_info":
            data = poll_system_info_once()
            self._send_json(data)
            return

        force = "1" in qs.get("force", [])
        with CACHE_LOCK:
            age = time.time() - CACHE_DATA.get("updated_at", 0)
            has_temp = CACHE_DATA.get("indoor_temp", "--℃") != "--℃"
        if force or age > 30 or not has_temp:
            data = poll_once()
        else:
            with CACHE_LOCK:
                data = dict(CACHE_DATA)
        self._send_json(data)


def main():
    parent_pid = 0
    if len(sys.argv) > 1:
        try:
            parent_pid = int(sys.argv[1])
        except Exception:
            parent_pid = 0

    threading.Thread(target=background_loop, args=(parent_pid,), daemon=True).start()

    try:
        server = HTTPServer(("127.0.0.1", 8766), HomeEnvHandler)
        server.serve_forever()
    except OSError:
        sys.exit(0)


if __name__ == "__main__":
    main()
