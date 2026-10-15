# 共用 Core 运行模式

```text
rustsimhub-core --serve-platform SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR MEDIA_SOCKET MEDIA_UID ARCHIVE_DIR ACCESS_DIR COMPAT_DIR NETWORK_JOURNAL_DIR
```

一个Core持有身份、业务库、短信观察/发送、电话控制与媒体授权、历史授权、NAS只读会话和有限Wi-Fi配置。Edge只连接这个Core；不能同时运行第二个Core共享业务库。目录与UID隔离要求沿用各模块规则，网络日志目录由Core拥有且0700。首次owner初始化仍走已有受限setup流程，不能从旧NAS认证库导入owner。

health显式报告messaging_api、call_control_api、history_read_api、legacy_read_api、wifi_provisioning_api。组合模式为authenticated_platform；媒体尚未完整交付时media_available仍为false。启用媒体授权不等于已验证网页声音或真实双向音频。

## 网络与电话边界

Wi-Fi配置和原保存连接试验共用排他网络锁；准备/提交新的拨号、接听和DTMF在执行期间持有共享语音锁。网络操作不能进入在途Core语音变更，语音变更也不能进入本Core的在途网络试验。请求不等待排队自动执行；忙时拒绝，尚未提交的电话意图保持原状态。

挂断、紧急挂断、租约失效清理、呼叫状态与结果查询不被网络锁阻断。原短信观察/读取、历史读取、NAS读取不持有网络写锁。网络本身仍在各个边界核验MM/NM唯一owner、无活动通话、USB与WWAN开关，不能靠一把内存锁替代硬件栅栏。

Core重启后的内存锁不能证明旧NM恢复点已结束。因此启用网络控制时，在准备/提交新的非挂断语音变更前，还会以有限超时检查当前NM不存在checkpoint；有恢复点或无法证明当前NM状态时拒绝。检查不销毁别人或旧Core的恢复点，不做网络重试。

底层超时可能存在未知结果，仍沿用电话持久意图/未知状态与仅查询不重发规则。这一协调只约束本Core请求，不能控制其他进程或运营商发起的来电；网络栅栏检测变化后执行自己的有限补偿。

## 验证方式

独立模拟D-Bus电话测试证明网络持锁期间不创建新意图/不消费原prepared意图/不调用CreateCall；释放后该意图可显式提交，网络持锁期间挂断仍完成。锁单元测试也覆盖反方向：在途语音请求阻止网络开始。

`tools/test-wifi-provisioning.py --platform --nas-adapter VERIFIED_ADAPTER_PATH` 使用一个真实生产Core，联合新配置保存/恢复、实际收件箱、合成历史、当前NAS授权与Go桥，禁止测试拨号/短信发送/蜂窝流量。实际结果应以独立VALIDATION记录和私有报告为准，不能由本文件的接口说明推定已验收。
