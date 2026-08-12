# -*- coding: utf-8 -*-
"""屏幕驱动小工具：截图 / 鼠标点击 / 按键 / 自动点推进。
用法（Git Bash，venv python）：
  python screenbot.py shot out.png            # 截全屏
  python screenbot.py click X Y               # 左键点击屏幕坐标
  python screenbot.py key F8                  # 按一个键
  python screenbot.py autopilot OUTDIR N      # 每 1.4s 点推进位，每 5s 存一帧，共 N 秒
"""
import ctypes, sys, time, os, subprocess

user32 = ctypes.windll.user32

def click(x, y):
    user32.SetCursorPos(int(x), int(y))
    time.sleep(0.05)
    user32.mouse_event(2, 0, 0, 0, 0)  # LEFTDOWN
    time.sleep(0.05)
    user32.mouse_event(4, 0, 0, 0, 0)  # LEFTUP

def key(vk):
    user32.keybd_event(vk, 0, 0, 0)
    time.sleep(0.05)
    user32.keybd_event(vk, 0, 2, 0)

def focus(title_part):
    """按标题子串把窗口置前。返回匹配的窗口标题，没找到返回 None。"""
    found = []
    WNDENUMPROC = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def cb(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            buf = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, buf, 256)
            if buf.value and title_part.lower() in buf.value.lower():
                found.append((hwnd, buf.value))
        return True
    user32.EnumWindows(WNDENUMPROC(cb), 0)
    if not found:
        return None
    hwnd, title = found[0]
    # 解除前台锁定再置前
    user32.keybd_event(0x12, 0, 2, 0)  # Alt up，绕过 SetForegroundWindow 限制
    user32.ShowWindow(hwnd, 9)  # SW_RESTORE
    user32.SetForegroundWindow(hwnd)
    return title

VK = {"F8": 0x77, "F9": 0x78, "ESC": 0x1B, "SPACE": 0x20, "ENTER": 0x0D}

PS_SHOT = r'''
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$w=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$b=New-Object System.Drawing.Bitmap $w.Width,$w.Height
$g=[System.Drawing.Graphics]::FromImage($b)
$g.CopyFromScreen(0,0,0,0,$b.Size)
$b.Save('__OUT__',[System.Drawing.Imaging.ImageFormat]::Png)
'''

def shot(path):
    # 直接把输出路径嵌进 PowerShell 命令（-Command 模式下 $args 传参不可靠）
    win_path = os.path.abspath(path).replace("/", "\\")
    cmdline = PS_SHOT.replace("__OUT__", win_path.replace("'", "''"))
    subprocess.run(["powershell", "-NoProfile", "-Command", cmdline], check=True)

if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "shot":
        shot(sys.argv[2])
    elif cmd == "click":
        click(sys.argv[2], sys.argv[3])
    elif cmd == "key":
        key(VK[sys.argv[2].upper()])
    elif cmd == "focus":
        print(focus(sys.argv[2]))
    elif cmd == "fclick":
        # 置前并立即点击（同一进程内，避免被用户操作抢焦点）
        t = focus(sys.argv[2])
        if not t:
            print("window not found"); sys.exit(1)
        time.sleep(0.3)
        click(sys.argv[3], sys.argv[4])
        print("clicked", t)
    elif cmd == "wininfo":
        # 打印窗口客户区矩形，算坐标用
        t = sys.argv[2]
        WNDENUMPROC = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
        hits = []
        def cb(hwnd, _):
            if user32.IsWindowVisible(hwnd):
                buf = ctypes.create_unicode_buffer(256)
                user32.GetWindowTextW(hwnd, buf, 256)
                if buf.value and t.lower() in buf.value.lower():
                    hits.append(hwnd)
            return True
        user32.EnumWindows(WNDENUMPROC(cb), 0)
        if hits:
            class RECT(ctypes.Structure):
                _fields_ = [("l", ctypes.c_long), ("t", ctypes.c_long), ("r", ctypes.c_long), ("b", ctypes.c_long)]
            rc = RECT()
            user32.GetWindowRect(hits[0], ctypes.byref(rc))
            print("rect", rc.l, rc.t, rc.r, rc.b)
        else:
            print("not found")
    elif cmd == "hoverclick":
        # 先悬停再点（有些 UI 需要 hover 事件）
        user32.SetCursorPos(int(sys.argv[2]), int(sys.argv[3]))
        time.sleep(0.4)
        click(sys.argv[2], sys.argv[3])
        print("hoverclicked")
    elif cmd == "list":
        # 列出所有可见窗口标题，找游戏窗口用
        WNDENUMPROC = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
        def cb(hwnd, _):
            if user32.IsWindowVisible(hwnd):
                buf = ctypes.create_unicode_buffer(256)
                user32.GetWindowTextW(hwnd, buf, 256)
                if buf.value:
                    print(buf.value)
            return True
        user32.EnumWindows(WNDENUMPROC(cb), 0)
    elif cmd == "autopilot":
        outdir, dur = sys.argv[2], float(sys.argv[3])
        os.makedirs(outdir, exist_ok=True)
        t0 = time.time(); last_frame = 0; i = 0
        # 推进点：屏幕中下部（对白点击区）
        sw, sh = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
        ax, ay = int(sw * 0.5), int(sh * 0.75)
        while time.time() - t0 < dur:
            click(ax, ay)
            if time.time() - last_frame >= 5:
                i += 1
                shot(os.path.join(outdir, "f%04d.png" % i))
                last_frame = time.time()
            time.sleep(1.4)
