# 电话加密媒体实机验收

2026-10-14，固定总体进度从 50% 到约 51%，P3 增加 1 点。

原生源码 93a4005，源包 c6bea29a3c122ad01d464c508bbe3eacec9a07980a7d7ad343cc4474601f704a；10 library + 1 binary 测试通过。原生 SHA256 ad18aab86765db4699b2013e0b3cb48eea752e5bbdd11eeb8337b081a6a6bf85。
core/ctl 为 de9e173 源包 a78207e3f5eacc618a2e09ac1bc10cc7d70d9b752773a5bd5e44198203cbd56b；开发 core 只用于私有独立库。

实际 UDP/ICE/DTLS 握手后，以真实远端证书 fingerprint 签发核心票据，按 UDS UID、scope、活动通话、SIM/后端 fence 续验，才打开 PCM。
三秒 PCM：captured 150，sent 150，received 156，played 149；capture_peak 13326，playback_peak 1，xrun 1，dropped 4，transport_failed=false。
NAS 收到并解码 150 帧，decoded_peak 15184；NAS 未打开 PCM，音频在内存解码并丢弃，未录音。NAS 上行是编码静音，1 LSB 为量化残差。

一次完成测试拨号并显式自动挂断，18 秒独立计时保护。此前另一次拨号因测试中间文件权限不符合私有文件检查提前停止，已只读核验结束，无自动重拨 Unknown。加上先前 12 次，当前共 14 次 10010 拨号。
另有一个准备阶段字段封装错误，在拨号前退出，没有消耗拨号。全部 once 保留，旧四服务 active，PCM 全 closed，无新增短信发送或普通蜂窝数据激活。
纯合成提前结束测试：棒子送 100 收 98，NAS 收 100；NAS peer_closed=true，棒子 revocation_closed=true，零 PCM、零拨号。

私有证据：.private/rtc-carrier-second.log、rtc-carrier-first.log、rtc-carrier-first-reconciled.log、rtc-trusted-early-close.log、device-rtc-trusted-selfcheck.log、rtc-trusted-native-tests.log。
可复用工具 tools/test-rtc-carrier.py 要求 --call-10010 和独立 --once-tag；不得重用已消费标记。其他号码/新短信不能从本授权推定。

边界：核心使用 Root，媒体使用旧 nassimhub 账户及有限音频设备允许列表。生产账户权限隔离、网页电话与声音、真实双向听音、来电及 TURN 尚未验收；media_available 仍 false，不能称完整电话交付。
