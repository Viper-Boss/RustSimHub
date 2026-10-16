# 实际 HTTPS 与 USB/Wi-Fi 媒体

2026-10-16，候选RootDirectory隔离实机验收。应用源874f01ca1bccc78d812f6683898f491942d861e1，归档ff1d445b31d600dfd106d83a27e2abce5ebbd55bc857d3036eedbc0dbff38aae；114构建输入。六程序本机ARM生产构建退出0，NAS同源Native20项/release退出0，Native5516704字节，SHA e6bf7769dda3a94c0413b5a0cd2682ae71e1ca22668672a073be6a7fb4a80247。本机201 workspace/20 Native及两者Clippy、Node媒体生命周期与proof验证通过。原签名9b应用和fbc整系统组合保持各自归属，未被此源码替换。

`tools/test-web-media-device.py`默认不拨号；只有显式`--call-10010 --once-tag UNIQUE`在持续授权下才提交一次10010。独立18秒Core停止保护先于提交启动，不明回复只观察同一操作，不自动重拨。所有实测凭据、SDP和硬件日志仅在私有目录。

| 私有报告 | 结果 |
|---|---|
| web-media-no-call-first.json | passed/cleanup_verified；严格受信任TLS、签名owner登录、无ACTIVE租约拒绝、跨Origin拒绝、HTTP stop_all及直接票据/precheck拒绝；三程序候选Root/exe/固定UID/空cap，重复Edge拒绝且原实例健康 |
| web-media-call-first.json | passed/cleanup_verified；USB一次10010，web-media-20261016-a1已消费；实际HTTPS prepare/poll/activate，真实DTLS/Opus/PCM；捕获/发送126、接收128、播放126、丢帧0、可恢复xrun1；NAS解码159帧/峰值17125 |
| web-media-wifi-first.json | passed/cleanup_verified；既有Wi-Fi一次10010，web-media-20261016-a2已消费；捕获/发送101、接收103、播放101、丢帧0、可恢复xrun1；NAS解码147帧/峰值16688、端侧丢帧2 |
| web-media-dual-no-call-first.json | passed/cleanup_verified；没有新通话，USB和Wi-Fi实际连接共用相同设备公钥、受信任证书名/Origin和同一个owner token |

两轮通话均验证隐私.local候选只映射到实际TLS peer、重复activate不再发票据；PCM活动时停止Edge，Core和Media仍存活而PCM关闭，NAS确认关闭。随后挂断、撤回临时Voice规则且恢复拒绝，所有自己的临时单元停止，旧nassimhub-agent/ModemManager/NetworkManager/rmtfs active、电话空闲、PCM全关闭。累计10010由20到22，新增短信/蜂窝/分区写入均0。

Wi-Fi报告保留了Edge刚启动尚未监听时的ConnectionRefused诊断；等待同一服务就绪后全部最终断言通过。后续工具将此类正常就绪等待存为readiness_diagnostics，不抹去原始报告。没有更换SSID或修改网络配置，没有普通蜂窝APN、互联网或回退测试。

这是程序化严格HTTPS和真实媒体检查，不是浏览器验收。Wi-Fi通过显式连接地址并严格校验证书名，未验证DNS/mDNS发现。未验证真实麦克风上行/人耳听音/浏览器权限、硬SIGKILL联动、长期稳定、来电、远程TURN或完整系统上电。先前浏览器同源8448访问被客户端拦截，不能换端口/接口/浏览器绕过。小量欠载/丢帧仍保留，不能宣称无音频故障。固定进度P3+0.5、P4+0.5，总体72.5/100约73%。
