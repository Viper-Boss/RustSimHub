# 开启配网热点前主动扫描：实现与交付边界

日期：2026-10-29。主线负责人范围，未接管其他成员任务。承接[应用69正常运行记录](VALIDATION-APP69-SETUP-RECOVERY-20261029.md)。本轮完成代码和离线验证，**尚未部署、尚未通过棒子真实扫描或手机配网验收**；设备继续运行已确认69，用户此前表示不方便测试，本轮不开热点。

## 问题与最终目标

原配网页读取的是NetworkManager已存在的AP缓存，按钮不能产生新扫描，所以只见保存过的SSID、反复读取不变化。此前仅修正文案不能替代扫描功能。本批实现“开热点前主动扫描→保护快照→当前Owner读取”；热点运行期间点击按钮仍只是读取这一轮列表。**用户要求的热点内显式重新扫描仍未实现**，不能把这一批写成全部扫描需求完成。

## 执行边界和接口

1. `crates/setup-ap/src/nm/scan.rs`：仅Root现有NetworkManager后端调用，固定成功预检得到的wlan0和唯一NM进程；每个NM实例/配网轮次至多一次`RequestScan`，选项恒为空，不接收SSID、设备路径、命令或调用者设置。不启用无线、不切换连接、不更改APN或蜂窝数据。
2. 扫描最多8秒，超时、拒绝、回复丢失都消耗本轮请求机会，不重试。方法成功返回不是完成证据，必须看到`LastScan`比扫描前更新且不早于请求时刻、不晚于当前CLOCK_BOOTTIME。NM进程更换、时间倒退/未来值、外来检查点、无线关闭、接口或原连接变化均拒绝继续。
3. 获取AP路径最多128个，拒绝重复或非AP路径；AP对象的`LastSeen`为秒精度，过滤未知、早于本轮所在秒或未来的对象。同一秒的历史观察无法细分，明确保留这个精度限制，不声称逐毫秒证明每条AP由本次请求独占发现。
4. 快照最多32项，按SSID/加密方式/频率去重、保留最强信号；只输出SSID、信号、MHz和有限加密类型。不输出BSSID、D-Bus路径、密码或连接配置。二进制/隐藏/控制字符/超长名称不作为选项；开放和不支持的加密类型不能冒记为可用WPA2。
5. `transaction::scan_before_start`接入普通AP事务和实际使用的configurable执行器，均在安装隔离/检查点/切换AP之前执行；扫描前后复核既有租约，不延长原截止时间。扫描不支持/拒绝/丢回复且网络上下文仍可证明安全时可继续手动SSID配网，但不产生主动扫描成功快照；身份/上下文改变或畸形数据保持失败。
6. `channel::scan::Proof`通过固定Root输出槽`scan-snapshot.json`发布，format=1、完整原Binding和有界Snapshot；校验boot、设备、轮次nonce、Core实例、材料与原截止时间，沿用受保护目录/0640原子写入及拒绝链接规则，不增加任意路径API。Root持久consume在扫描之前，重启恢复不会重发扫描。
7. Core只在当前轮次仍Ready、未取消、Root最新Ready回执有效且Proof精确绑定时读取；Owner接管沿用同一原Binding，因此不会丢掉本轮结果。重启、关闭、Unknown、过期、另一设备/轮次或链接内容不作为新快照。
8. 仍通过原Owner限定的`wifi_networks`只读接口展示，读取前后Owner/NetworkManage权限检查不变。新source为`before_setup_ap_scan`并提供快照年龄；`scan_requested=false`和`hardware_mutations=false`描述这次**读取请求**，不否认Root此前独立执行过扫描。成功的空扫描也覆盖旧缓存，不用旧SSID伪造新结果。
9. 网页区分主动扫描快照、开热点前普通缓存和NM缓存，按钮为“读取Wi-Fi列表”，说明热点期间读取不触发扫描。不改变短信、电话、配对、流量和其他成员接口。

语义按官方[Wireless RequestScan/LastScan](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.Device.Wireless.html)和[AccessPoint LastSeen](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AccessPoint.html)核对。代码使用有界轮询读取新的LastScan，不依赖zbus属性缓存。

## 当前验证

- 初始底层：setup-ap 115项、0忽略，严格Clippy/fmt通过。归档`bf58f841625072a192ec7008e5601a54dbe7902b11403ddff4a864fef94967dd`。
- 最终相关Core/Edge/setup-ap：**331项通过、0忽略**，严格Clippy/fmt通过；归档`48da32f306552a22bdee6c3476e724f7e429aaf174c3e870023f904fa98db314`，日志摘要`26d77cf760430b2fffa1d9d36b9b9690da859f732f8f0d5cfb071bc737aa9a24`。UID65534、无网络/设备、空capabilities，所有D-Bus服务都是明确的Unix socket模型，没有访问系统总线或真实无线设备。
- 覆盖一次真实D-Bus方法调用、完成标记、拒绝/丢回复/未完成、NM更换、连接变化、未来时间、扫描中外来检查点/关闭无线、恶意AP路径、受保护发布的撤销与链接拒绝、跨轮次Proof、Owner接管保持、空扫描替代旧缓存。模型测试证明代码路径，不能证明wcn36xx真机行为。
- **18组网页回归全部通过**，包括账户、配网、Wi-Fi选择、会话、短信、通话和转发；新增主动快照年龄、空扫描、来源/年龄畸形值和无额外扫描命令检查。属于程序化模拟，未宣称手机视觉或真实配网通过。
- ARM候选构建和验签将在冻结源码后追加；此处不能把主机测试写成已完成ARM或设备安装。

## 接手动作

1. 冻结当前源码，交叉构建并核对Core、Edge和配网Root助手；沿用当前信任签名并验证候选，不自动安装或开启热点。
2. 用户方便时再准备有界真实数据恢复和USB维护，配对更新应用与Root助手；一键开始新的隔离配网轮次，实际观察RequestScan/LastScan、附近SSID和列表年龄，分别验证空结果/拒绝不冒记新列表。
3. 完成正确/错误密码、返回热点、明确保存和热点退出的整链路。未知结果保存现场，不自动重放。未验收前AP-07/08不标完成。
4. 另行设计热点内显式重新扫描及断开/重连体验；当前只读按钮不会执行该动作，不能据此回答“已经支持随时刷新扫描”。

原P0–P7仍约81.5%，行动台账20/169（加权10.6%），本批不加验收点；D任务继续暂停。原始测试输出和候选材料只放忽略的私有目录。
