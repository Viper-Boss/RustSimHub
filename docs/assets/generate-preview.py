"""Generate dependency-free, editable README concept artwork (not live telemetry)."""
from pathlib import Path
from html import escape

ROOT = Path(__file__).resolve().parent
BG, PANEL, LINE = '#0a101c', '#111c2e', '#25334a'
WHITE, MUTED, CYAN, GREEN = '#eef5ff', '#91a2bb', '#65ddd4', '#92e5b2'

# Keep product names and protocol abbreviations; localize all user-facing copy.
ZH = {
    'Your SIM. Your device. Your control.': '你的 SIM，你的设备，由你掌控。',
    'An independent communication system for MSM8916.': '面向 MSM8916 的独立 SIM 通信系统',
    'SMS  /  VOICE  /  LOCAL-FIRST  /  OPTIONAL NAS': '短信 / 语音 / 本地优先 / 可选 NAS 接入',
    'DEVICE / UFI003': '设备 / UFI003', 'Small device.': '小巧设备。', 'Independent core.': '独立通信核心。',
    'UI CONCEPT / SAMPLE DATA': '未来界面概念 / 示例数据', 'COMMUNICATION CONSOLE': '独立通信控制台',
    'Overview': '总览', 'Messages': '短信', 'Phone': '电话', 'Network': '网络', 'Forwarding': '短信转发',
    'Access & security': '访问与安全', 'System': '系统', 'Local connection': '本地连接',
    'Your communication, at a glance.': '通信状态，一目了然。',
    'A calm home for your SIM, messages and calls.': '在同一处管理设备、短信与电话。',
    'Owner session': '所有者会话', 'DEVICE': '设备状态', 'Online': '在线',
    'UFI003 · local HTTPS': 'UFI003 · 本地 HTTPS', 'SIM / RADIO': 'SIM / 无线网络',
    'LTE registered': 'LTE 已注册', 'Example carrier · sample status': '示例运营商 · 示例状态',
    'MESSAGES': '短信', '8 unread': '8 条未读', 'All content shown is fictional': '展示内容均为虚构示例',
    'CELLULAR DATA': '蜂窝数据', 'Off': '已关闭', 'No automatic cellular fallback': '不自动回退至蜂窝数据',
    'Recent messages': '最近短信', 'View inbox →': '查看收件箱 →', 'Demo service': '示例服务',
    'Your example verification code is 123456.': '你的示例验证码为 123456，请勿分享。',
    'Carrier notice': '运营商通知', 'A fictional service reminder for this preview.': '这是一条用于界面预览的虚构服务提醒。',
    'Sample contact': '示例联系人', 'Everything is ready. See you later.': '一切准备就绪，稍后见。',
    'Yesterday': '昨天', 'Idle': '空闲', 'Ready when you are.': '随时准备好沟通。',
    'Select a number to prepare a call.': '输入号码，准备发起通话。', 'Enter phone number': '请输入电话号码',
    'Open dialer': '打开拨号盘', 'Connection paths': '连接方式', 'Optional bridge': '可选桥接',
    'Local network': '局域网连接', 'Planned · off': '规划中 · 已关闭',
    'Local access and remote access are verified separately.': '本地访问与远程访问分别验证。',
    'Extensions': '扩展功能', 'SMS forwarding': '短信转发', 'Planned': '规划中',
    'Usage reporting': '使用统计', 'Opt-in': '单独授权', 'Each extension has its own switch.': '每个扩展均有独立开关。',
    'DESIGN PREVIEW ONLY · NOT THE CURRENT SHIPPED WEB UI · NO LIVE DEVICE CONNECTION': '仅为未来设计预览 · 非当前已交付网页 · 未连接真实设备',
    'THE SAME DEVICE. A SMALLER SCREEN.': '同一台设备，适配更小的屏幕。',
    'Designed to travel with you.': '把通信控制台，装进口袋。',
    'Responsive web concept / no native app yet': '响应式手机网页概念 / 原生 App 尚未开发',
    'Device online': '设备在线', 'MY COMMUNICATION DEVICE': '我的通信设备',
    'Wi-Fi connected · cellular data off': 'Wi-Fi 已连接 · 蜂窝数据已关闭',
    'Unread messages': '未读短信', 'Phone status': '电话状态',
    'Everything in one place.': '日常通信，尽在一处。', 'Read messages. Manage calls.': '查看短信，管理电话。',
    'Choose how your device connects.': '自由选择设备的连接方式。', 'Search messages': '搜索短信',
    'Your example code is 123456.': '你的示例验证码为 123456。',
    'A fictional service reminder.': '这是一条虚构的服务提醒。', 'Everything is ready.': '一切准备就绪。',
    'Example sender': '示例发送者', 'Only sample data appears here.': '此处仅展示虚构示例数据。',
    'Enter a number': '请输入号码', 'Prepare call': '准备拨号',
    'Home     Messages     Phone     Settings': '总览　　　短信　　　电话　　　设置',
    'FUTURE UI CONCEPT · FICTIONAL DATA · RESPONSIVE WEB, NOT A NATIVE APP RELEASE': '未来界面概念 · 数据均为虚构 · 响应式网页，非已发布原生 App',
    'RustSimHub — independent SIM communication': 'RustSimHub — 独立 SIM 通信系统',
    'Future desktop console concept': '未来桌面控制台概念图',
    'Future mobile web concept': '未来手机网页概念图',
    'Project banner. Linux and Rust user space on MSM8916. SMS, voice and optional NAS integration.': '项目横幅：MSM8916 平台，Linux 与 Rust 用户态，短信、语音和可选 NAS 接入。',
    'Static future design with fictional messages and metrics. Overview, SMS, phone, network, security and independent extension switches. Not an implemented UI.': '未来静态设计，短信及指标均为虚构。包括总览、短信、电话、网络、安全及独立扩展开关。尚未实现此界面。',
    'Three mobile layouts for overview, inbox and dialing. All data fictional. Responsive web proposal; native mobile app has not been developed.': '总览、收件箱与拨号三个手机布局，全部为虚构数据。响应式网页提案，原生手机客户端尚未开发。',
}

def rect(x, y, w, h, fill=PANEL, r=16, stroke=LINE):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{fill}" stroke="{stroke}"/>'

def text(x, y, value, size=16, color=WHITE, weight=400):
    value = ZH.get(value, value)
    return f'<text x="{x}" y="{y}" font-size="{size}" fill="{color}" font-weight="{weight}">{escape(value)}</text>'

def pill(x, y, value, color=CYAN, w=130):
    return rect(x, y, w, 30, '#142b36', 15, '#284851') + text(x+14, y+20, value, 12, color, 600)

def svg(name, w, h, body, title, desc):
    content = f'''<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" role="img" aria-labelledby="title desc">
<title id="title">{escape(ZH.get(title, title))}</title><desc id="desc">{escape(ZH.get(desc, desc))}</desc>
<defs><radialGradient id="glow"><stop stop-color="#1f5862" stop-opacity=".42"/><stop offset="1" stop-color="#0a101c" stop-opacity="0"/></radialGradient></defs>
<rect width="{w}" height="{h}" rx="24" fill="{BG}"/>
<g font-family="Microsoft YaHei, PingFang SC, Noto Sans CJK SC, Segoe UI, sans-serif">{body}</g></svg>'''
    (ROOT/name).write_text(content, encoding='utf-8')

def banner():
    b = '<ellipse cx="1000" cy="150" rx="390" ry="280" fill="url(#glow)"/>'
    b += pill(48, 36, 'LINUX + RUST', w=142)
    b += text(48, 137, 'RustSimHub', 64, WHITE, 700)
    b += text(51, 182, 'Your SIM. Your device. Your control.', 24, CYAN)
    b += text(51, 227, 'An independent communication system for MSM8916.', 16, MUTED)
    b += text(51, 280, 'SMS  /  VOICE  /  LOCAL-FIRST  /  OPTIONAL NAS', 13, MUTED, 600)
    b += rect(858, 48, 280, 224, '#101e2c', 24, '#345366')
    b += text(882, 83, 'DEVICE / UFI003', 12, MUTED, 600)
    b += text(882, 133, 'Small device.', 25, WHITE, 600)
    b += text(882, 169, 'Independent core.', 25, CYAN, 600)
    for i, h in enumerate([9, 16, 25, 35, 46]):
        b += rect(884+i*22, 237-h, 11, h, CYAN, 3, CYAN)
    b += text(1020, 233, 'ARM64', 13, MUTED)
    svg('hero.svg', 1200, 320, b, 'RustSimHub — independent SIM communication', 'Project banner. Linux and Rust user space on MSM8916. SMS, voice and optional NAS integration.')

def desktop():
    b = text(34, 43, 'R / S', 23, CYAN, 700) + text(1195, 40, 'UI CONCEPT / SAMPLE DATA', 11, MUTED)
    b += rect(20, 65, 205, 735, '#0d1625', 18)
    b += text(42, 108, 'RustSimHub', 23, WHITE, 700) + text(42, 133, 'COMMUNICATION CONSOLE', 10, MUTED)
    for i, label in enumerate(['Overview', 'Messages', 'Phone', 'Network', 'Forwarding', 'Access & security', 'System']):
        y = 181+i*54
        if i == 0: b += rect(33, y-26, 179, 41, '#183943', 9, '#2c565e')
        b += text(49, y, label, 15, CYAN if i == 0 else MUTED, 600 if i == 0 else 400)
    b += text(43, 715, 'UFI003 / MSM8916', 12, MUTED) + pill(40, 738, 'Local connection', w=156)
    b += text(257, 106, 'Your communication, at a glance.', 30, WHITE, 600)
    b += text(258, 135, 'A calm home for your SIM, messages and calls.', 15, MUTED)
    b += pill(1204, 85, 'Owner session', w=145)
    cards = [('DEVICE', 'Online', 'UFI003 · local HTTPS', GREEN), ('SIM / RADIO', 'LTE registered', 'Example carrier · sample status', CYAN), ('MESSAGES', '8 unread', 'All content shown is fictional', WHITE), ('CELLULAR DATA', 'Off', 'No automatic cellular fallback', MUTED)]
    for i, (label, value, sub, color) in enumerate(cards):
        x = 257+i*280
        b += rect(x, 164, 263, 125) + text(x+20, 194, label, 11, MUTED, 600)
        b += text(x+20, 233, value, 24, color, 600) + text(x+20, 264, sub, 11, MUTED)
    b += rect(257, 311, 684, 271) + text(280, 347, 'Recent messages', 20, WHITE, 600) + text(826, 346, 'View inbox →', 12, CYAN)
    for i, (sender, msg, time) in enumerate([('Demo service', 'Your example verification code is 123456.', '14:32'), ('Carrier notice', 'A fictional service reminder for this preview.', '12:18'), ('Sample contact', 'Everything is ready. See you later.', 'Yesterday')]):
        y = 390+i*70
        b += rect(279, y-22, 38, 38, '#203548', 12, '#203548') + text(291, y+3, sender[0], 15, CYAN, 600)
        b += text(332, y-3, sender, 14, WHITE, 600) + text(332, y+20, msg, 12, MUTED) + text(849, y-2, time, 11, MUTED)
    b += rect(960, 311, 400, 271) + text(984, 347, 'Phone', 20, WHITE, 600) + pill(1240, 326, 'Idle', w=91)
    b += text(984, 397, 'Ready when you are.', 23, WHITE, 600) + text(984, 425, 'Select a number to prepare a call.', 13, MUTED)
    b += rect(984, 450, 350, 43, '#0b1422', 9) + text(1002, 477, 'Enter phone number', 14, MUTED)
    b += rect(984, 511, 350, 43, CYAN, 10, CYAN) + text(1104, 539, 'Open dialer', 14, BG, 700)
    b += rect(257, 603, 684, 174) + text(281, 640, 'Connection paths', 20, WHITE, 600)
    for i, (label, val, color) in enumerate([('USB / NAS', 'Optional bridge', MUTED), ('Wi-Fi', 'Local network', GREEN), ('NKNGuard', 'Planned · off', MUTED)]):
        x = 281+i*216
        b += text(x, 684, label, 13, MUTED) + text(x, 717, val, 18, color, 600)
    b += text(281, 751, 'Local access and remote access are verified separately.', 12, MUTED)
    b += rect(960, 603, 400, 174) + text(984, 640, 'Extensions', 20, WHITE, 600)
    b += text(984, 678, 'SMS forwarding', 14) + text(1234, 678, 'Planned', 12, MUTED)
    b += text(984, 710, 'Usage reporting', 14) + text(1234, 710, 'Opt-in', 12, MUTED)
    b += text(984, 748, 'Each extension has its own switch.', 12, MUTED)
    b += text(258, 807, 'DESIGN PREVIEW ONLY · NOT THE CURRENT SHIPPED WEB UI · NO LIVE DEVICE CONNECTION', 11, MUTED)
    svg('console-concept.svg', 1380, 830, b, 'Future desktop console concept', 'Static future design with fictional messages and metrics. Overview, SMS, phone, network, security and independent extension switches. Not an implemented UI.')

def mobile():
    b = text(46, 55, 'THE SAME DEVICE. A SMALLER SCREEN.', 13, CYAN, 600)
    b += text(46, 105, 'Designed to travel with you.', 33, WHITE, 600)
    b += text(46, 142, 'Responsive web concept / no native app yet', 17, MUTED)
    for x in [48, 458, 868]: b += rect(x, 180, 360, 586, '#111b2c', 30, '#344258')
    b += text(76, 219, '9:41', 13, WHITE, 600) + text(315, 219, 'LTE  ▰', 12, MUTED)
    b += text(76, 267, 'Overview', 27, WHITE, 600) + pill(76, 285, 'Device online', w=139)
    b += rect(72, 336, 312, 112, '#172b38', 18, '#284855') + text(92, 366, 'MY COMMUNICATION DEVICE', 10, MUTED, 600)
    b += text(92, 402, 'RustSimHub', 27, WHITE, 600) + text(92, 427, 'Wi-Fi connected · cellular data off', 12, CYAN)
    for i, (value, label) in enumerate([('8', 'Unread messages'), ('Idle', 'Phone status')]):
        x = 72+i*162
        b += rect(x, 465, 150, 98) + text(x+18, 506, value, 28, CYAN, 600) + text(x+18, 539, label, 11, MUTED)
    b += text(76, 602, 'Everything in one place.', 19, WHITE, 600) + text(76, 633, 'Read messages. Manage calls.', 13, MUTED)
    b += text(76, 657, 'Choose how your device connects.', 13, MUTED)
    b += text(486, 219, '9:41', 13, WHITE, 600) + text(486, 267, 'Messages', 27, WHITE, 600)
    b += rect(482, 287, 312, 40, '#0c1422', 10) + text(500, 313, 'Search messages', 13, MUTED)
    for i, (sender, msg) in enumerate([('Demo service', 'Your example code is 123456.'), ('Carrier notice', 'A fictional service reminder.'), ('Sample contact', 'Everything is ready.'), ('Example sender', 'Only sample data appears here.')]):
        y = 369+i*79
        b += text(486, y, sender, 15, WHITE, 600) + text(486, y+25, msg, 12, MUTED)
        b += f'<path d="M486 {y+42}h304" stroke="{LINE}"/>'
    b += text(896, 219, '9:41', 13, WHITE, 600) + text(896, 267, 'Phone', 27, WHITE, 600)
    b += text(962, 323, 'Enter a number', 20, MUTED)
    for row, keys in enumerate(['123', '456', '789', '*0#']):
        for col, key in enumerate(keys):
            x, y = 928+col*96, 382+row*68
            b += f'<circle cx="{x}" cy="{y}" r="27" fill="#1c2b40"/>' + text(x-7, y+8, key, 22)
    b += rect(899, 644, 297, 43, CYAN, 12, CYAN) + text(1003, 672, 'Prepare call', 14, BG, 700)
    for x in [48, 458, 868]:
        b += text(x+30, 730, 'Home     Messages     Phone     Settings', 11, MUTED)
    b += text(48, 807, 'FUTURE UI CONCEPT · FICTIONAL DATA · RESPONSIVE WEB, NOT A NATIVE APP RELEASE', 12, MUTED)
    svg('mobile-concept.svg', 1280, 840, b, 'Future mobile web concept', 'Three mobile layouts for overview, inbox and dialing. All data fictional. Responsive web proposal; native mobile app has not been developed.')

if __name__ == '__main__':
    ROOT.mkdir(parents=True, exist_ok=True)
    banner(); desktop(); mobile()
