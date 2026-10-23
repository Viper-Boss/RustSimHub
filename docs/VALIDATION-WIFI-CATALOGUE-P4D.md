# P4D 网页 Wi-Fi 选网

2026-10-24。已交付185ec6e/confirmed43：固定508份Git文件同源构建、267项Rust、八ARM程序、同源Native33项、12组网页及CI40022101743通过。原信任密钥签名和设备验签/包回读、一致性备份及业务回读通过；九份严格HTTPS资源与设备身份匹配，原Owner/67短信/Unknown操作、Root音频/网络保持。相同boot无恢复预算消耗，不重启、不刷分区、不扫描/切网、不新拨号或发送短信，普通数据关闭。真实Owner缓存读取、网页视觉及切网尚未验收，整体79.5%，不增加验收分数。

## 实际交付

只选择并确认第43组应用；原40组、factory16及此前应用组保留。Core/Edge/Media仍以原受限用户运行，能力集为空；七项音频路由保持开启、PCM关闭。正常升级没有调用关机驱动，既有PMIC/SMD问题继续搁置。

新wifi_networks及既有两项会话续期接口在严格HTTPS下拒绝无效凭证；这验证了实际新接口接线和拒绝路径，不能代替有权Owner成功读取缓存。原业务数据完整性检查通过，已存在的终止Call重观测另记，不把新增历史行当新拨号。

最新私有索引：`.private/wifi-catalogue-checkpoint-20261024-first.json`。分阶段preflight/stage/backup/start/health/confirm报告及原始构建日志保留。Native容器已正常退出并移除，源码/日志/产物/cache保留；Windows临时证书继续移除状态。

## 使用方式

“网络状态、流量与配置”折叠区内提供Wi-Fi配置。所有者可在同区重新登录，点击“读取已发现的网络”，看到网络名、当前连接标志、信号百分比、频率和支持的加密类型。选择兼容网络只填写SSID并清空旧密码，光标移到密码框；仍由用户点击试连接、查询、确认保存或取消。隐藏网络可手输。

列表来自NetworkManager已经持有的缓存，页面明确说明没有请求新扫描。没有缓存时保留手输入口，不能把“刷新列表”显示成刚刚扫描成功。WPA2个人网络可选；开放、WEP、WPA1-only、企业认证和WPA3-only不在当前配网范围内，不能退化为开放连接。列表中的选择只提供SSID，不锁定或证明某个BSSID。

已有40秒确认期限、NM自身45秒checkpoint、语音/网络互斥、USB备用连接、保存和失败回退规则保持。Wi-Fi试连接的查询失败现在保留原trial_id并禁用确认，允许再次查询，不再因读取失败清空试连接编号。本轮没有新增自动连接、确认、写重试或普通蜂窝回退。

## 接口与边界

新增协议2命令`wifi_networks { token }`，无设备路径、扫描参数、SSID筛选、设置字典或读取口令选项。现有严格HTTPS/IPC承载，schema6保持，不新增polkit或内核权限。

Core要求当前有效会话、最近5分钟内的真实Owner和network.manage；委派了network.manage的普通Client仍不能读取。读取完成后重新核验同一principal/generation/scopes与最近登录条件。单并发、5秒读取截止；失去授权不返回网络名。

固定NetworkManager唯一owner，最多16个设备，只接受一个被管理的无线设备；最多128个AP路径，拒绝重复或外部路径。读取前后直接取LastScan，发生变化则明确失败，不自动重新扫描或重读。最多输出32个显示选项，按当前连接、信号和稳定字段排序；同SSID/加密/频率的多个AP仅合并显示选项，不涉及业务记录去重。

输出只含有界UTF-8名称、0至100信号、MHz频率、有限加密标签、可选/当前连接标志及缓存说明。不返回MAC/BSSID、NM路径、连接UUID、保存配置或口令。空、非法UTF-8、控制字符、超过32字节的SSID不转换成另一个可连接名称。所有名称用textContent渲染。

网页在退出、重新登录、重新读取后隔离旧列表和旧选择按钮；整个目录验证成功后才创建选择入口。读取失败清理旧列表，并提示同区重新登录或手输。通话期间沿用当前主线动作互斥。

## 已验证

- 267项Rust工作区测试、格式化、全工作区严格Clippy与Core无开发功能生产检查通过。
- 新增真实p2p D-Bus夹具，服务没有扫描、激活或设置写方法；正常缓存读取通过，owner变更、LastScan变化、外部路径失败。
- 真实Auth检查：无效会话和委派Client在读取前拒绝；读取期间身份实例重建后拒绝返回。真实IPC拒绝无效会话，不依赖模拟鉴权。
- 12组网页回归通过。新组执行生产app，验证文本显示、选择不发连接命令、支持范围、无缓存、完整目录验证、退出/换代/迟到回复、读取失败和保留试连接编号。
- CI新增对应网页测试。实际CI结论与源码SHA记入私有source-checkpoint，不能用本地通过代替远端结果。

私有记录：`.private/wifi-catalogue-workspace-20261024-fourth.log`及`.exit`、`.private/wifi-catalogue-web-20261024-first.json`。首次夹具因私有目录权限缺失失败、第二次严格Clippy指出可合并分支，均已修正；首份失败报告保留，最终第四份为权威结果。

## 尚未完成

新接口的真实Owner缓存读取、真实浏览器视觉/选择及试连接/错密码/超时回退验收。当前主线不安排用户休息期间的网络切换。

本功能不是主动扫描或首配热点。受保护AP、无保存SSID首配、备用维护入口、设备发现及可信证书交付仍按[独立配网前置检查](WIFI-STANDALONE-READINESS-2026-10-24.md)推进；不能据此宣称无NAS首次配网已完成。

## 源码与依据

- Core：`crates/core/src/network/wifi.rs`及`wifi/tests.rs`，接线`authenticated.rs`；协议`crates/wire/src/auth.rs`。
- 网页：`crates/edge/src/web/app.js`、`index.html`、`style.css`；回归`tools/test-web-wifi.mjs`。
- NetworkManager官方：[无线设备缓存与LastScan](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html)、[AP属性](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AccessPoint.html)、[加密标志定义](https://networkmanager.dev/docs/api/latest/nm-dbus-types.html)。
