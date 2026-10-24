# BLE应急配网与Web Bluetooth配置器：任务队列

2026-10-24，按用户“把蓝牙也利用一下，加到任务里”纳入开发。原文保存在[用户任务书](REQUEST-2026-10-24.md)，它是需求输入及验收标准，不是现有实现或真机能力证明。本轮仅完成需求接收、源码基线核对及工作拆分。

BLE独立开发分支为`codex/ble-web-configurator`，从已完成本地345项回归的Root执行循环主线f255136创建。本分支当前只有任务文档；后续BLE实现继续在独立分支验证。任务索引同步回主线，避免接手遗漏。没有委派给其他代理或Claude，也没有假定他们在开发任何文件。

## 目标

已预先获Owner授权的Windows Chrome/Edge或Android Chrome浏览器，通过BLE恢复身份和加密GATT会话，在usb0不存在且Wi-Fi断开时，调用同一个Core的有限Wi-Fi事务进行恢复。网页位于独立`configurator/`，计划通过GitHub Actions发布Pages，并支持首次联网缓存后的PWA离线打开。

BLE仅用于有限状态和配网；不传KVM视频，不提供短信/电话、shell、任意文件、OTA、普通APN、Owner重置或通用AuthCommand透传。首次版本不开发iPhone原生适配、APK、EXE或小程序。GitHub Pages不是设备后端，不接收口令或会话秘密。

## 基线核对及未验证项

- 当前Core是唯一业务/SQLite权威；已有网络锁和电话协调，`wifi_begin/status/confirm/cancel`保留最近Owner/network.manage、40秒Core/45秒NM及原USB栅栏。不能仅删除usb0检查。
- 当前工作区没有`crates/ble`、`configurator/`或BLE Pages工作流。已有Rust CI必须保留；现有AP/STA文件通道不等于BLE加密/认证。
- UFI003板载HCI、LE Peripheral、Advertising、BlueZ本地GATT注册及实际RF/天线均**未核实**。不能因为MSM8916家族能力就判定这根棒子可用。
- 本轮没有为BLE连接或修改生产棒子，没有安装BlueZ/服务、加载驱动、广播、配对、改DTB/内核/NV、切USB模式、重启或刷写；没有Pages部署。
- 本轮另有已授权短信测试的只读生产观测，与BLE能力探测授权无关。此前宽泛设备调试许可不替代本任务书限定的BLE实机授权。

## 工作包与验收证据

| 包 | 工作 | 必须留下的证据 | 当前状态 |
|---|---|---|---|
| B0 | 审核现有离线内核配置/设备树/固件及BlueZ记录，形成板级能力矩阵；缺证据列出需授权的只读项目 | 固定源码/快照版本、HCI/外设/广播/GATT/RF各项证据来源；未知不得填支持 | 待实施；仅完成任务接收 |
| B1 | 固定版本化GATT UUID、有限帧/分片、超时/重放、设备签名及标准ECDH/HKDF/AEAD会话 | 纯Rust与浏览器算法可用性/互操作、失败向量；不自行发明加密方案 | 待实施 |
| B2 | Owner近期授权独立恢复公钥、狭窄wifi.recovery能力、撤销/generation；同Core强类型可信BLE通道 | 未认证/未授权无USB拒绝、合法BLE无USB通过、原USB不变，开始/确认/保存后再次核验 | 待实施 |
| B3 | 低权限rustsimhub-ble、BlueZ GATT/Advertising、独立限权IPC/SO_PEERCRED、单会话资源预算 | Mock BlueZ/NM及恶意帧/断连/服务崩溃测试；不能读取业务库/identity seed；无BLE时Core照常运行 | 待实施 |
| B4 | 原生JS/CSS的Windows/Android配置网页、可信设备pin/独立非导出恢复密钥、状态/确认/取消及PWA | 合成GATT/加密接线、无自动重发、只缓存静态公开资源、相对路径/升级/离线验证 | 待实施 |
| B5 | 独立ble-configurator-pages.yml、静态artifact和部署手册 | 现有CI不回归、真实部署结果/HTTPS站点；未成功前不宣称预期URL上线 | 待实施 |
| B6 | 单独授权后的HCI/RF/Windows/Android互操作、无USB+Wi-Fi断开恢复、错误密码/断连/撤销与业务保留 | 实际广播/GATT/配网/回退和原Owner/身份/证书/短信/电话不变的独立报告 | 待授权、待前置就绪 |

每个包按源码实现、离线通过、硬件通过、应急配网通过、Pages上线分别记录，不能互相代替。恢复密钥应在正常可信管理可用时提前绑定；没有提前绑定不允许匿名成为Owner。不复用9008按钮作为未验证的授权按钮。

## 接手顺序与冲突

先完成当前AP主线的Core授权生产者/保存/网页闭环，再开始B0/B1的离线部分；若用户调整优先级则据此变更。B2涉及现有network_trial/provision/authenticated/Auth/Policy，必须基于届时最新HEAD重新核对，避免覆盖正在推进的AP网络锁和Owner边界。BLE独立限权通道不是“前端传channel=ble”，不能创建第二个共享业务库的Core。

真机BLE门禁来自用户任务书：“未经单独授权不连接或改动生产棒子”。需要现场能力检查时先把检查项、备份/恢复边界准备具体再询问；不因硬件未知停止可做的离线实现，不因用户不在场冒险操作。

用户另明确要求：能直接调试浏览器的任务由代理直接操作，确实做不了时才请用户协助；该偏好已写入主线AGENTS。本轮电脑操作因工具无法可靠确认浏览器当前URL而自动终止，不能用其他UI技术绕过；这不等于BLE、固件源码或普通离线开发被禁止。

## 进度口径

原P0–P7仍按[固定里程碑](../PROGRESS-SCALE.md)报告**79.5%**。BLE扩展单独追踪，目前仅任务纳入，功能尚未实现、硬件未验证、Pages未部署。原主线达到100%不自动代表BLE扩展完成；最终交接必须同时核对这份新增队列。
