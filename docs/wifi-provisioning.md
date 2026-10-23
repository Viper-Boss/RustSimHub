# 新 Wi-Fi 配置

2026-10-24补充：[P4D选网入口](VALIDATION-WIFI-CATALOGUE-P4D.md)源码已完成，尚未交付或实机验收。新增所有者`wifi_networks`只读缓存列表；不请求扫描，不改变现有连接。网页配网移动到网络配置折叠区，提供同区重新登录、名称/信号/频率/加密选择及隐藏网络手输。原试连接/确认/回退规则保持。

Core显式启用 `--serve-network-provisioning SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR JOURNAL_DIR`；其他模式拒绝新配置命令。JOURNAL_DIR须是Core拥有的私有目录。现有严格HTTPS/v2授权入口承载请求，不增加任意shell、NM设置字典或AT接口。

需要短信/电话/历史/NAS与配网同时提供时，使用[platform-runtime.md](platform-runtime.md)的`--serve-platform`，同一Core持有业务库和网络协调锁。独立配网模式仍保留用于隔离验收。

所有操作要求5分钟内重新登录的owner和network.manage，且当前公钥/generation/scope仍匹配。只配置wlan0的WPA2基础设施模式；SSID为1至32字节UTF-8，不含控制字符，密码为8至63字节可打印ASCII或64位十六进制PSK。不提供开放热点、普通APN、全局网络/无线开关或自由接口选择。现阶段须保持usb0已连接，支持原Wi-Fi已连接或设备已管理但未连接的首次设置。

| op | 字段 | 行为 |
|---|---|---|
| wifi_begin | token、idempotency_key、ssid、passphrase | 发布持久一次意图，立即返回trial_id；后台创建仅Wi-Fi的45秒NM恢复点，并以volatile、dbus-client、autoconnect=false创建独立新配置 |
| wifi_status | token、trial_id | 查看starting/connecting/ready/saving/saved/rolled_back/unknown与剩余秒数，不返回SSID或密码 |
| wifi_confirm | token、trial_id | 仅ready可确认；再次核验当前owner、NM/MM唯一owner、USB、WWAN开关、无活动通话、实际AP的SSID、当前候选profile和IP，再记录确认意图、保存自己的配置并销毁自己的恢复点 |
| wifi_cancel | token、trial_id | 恢复自己的NM checkpoint、删除自己的候选配置，核对原连接；失败保持unknown，不自动重新应用 |

Core的40秒期限比NM恢复点更短。未确认、取消、失去权限或错误配置会执行补偿，Core进程消失后仍有NM限时恢复及dbus-client/volatile清理。普通蜂窝自动连接配置存在时拒绝开始，防止配置失败触发流量回退；不关闭语音所需WWAN/IMS。

确认时Update2仅写由程序构造的有限Wi-Fi设置，保存autoconnect=true；NM在断开后可自行重连，无需NAS。持久确认意图先于写盘，保存后的崩溃不能冒称没有保存；未知结果不自动重试。保存过程失败时仍尝试恢复原连接并删除自己的配置，恢复无法证明时返回unknown。

同一次在内存中的begin重复请求只有相同owner/generation/完整输入摘要才返回原状态。持久开始标记阻止Core重启后再次激活同一key。密码不写Core日志/意图或网页IndexedDB；只有显式确认后由NM保存到标准受保护的连接存储。网页有试连接、查询、确认和取消，退出清空密码和临时状态；没有自动确认或网络写重试。

底层语义参考[NetworkManager管理器API](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html)、[连接Update2](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Settings.Connection.html)。实现/模拟测试不代替目标棒子的真实保存、重启和故障恢复验收；网页还需独立浏览器验证。
