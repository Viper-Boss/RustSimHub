# A-17：公开源码入口与权限边界盘点

固定主线SHA：`80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d`。本表逐项只读核对当前入口和相关调用链；文件摘要附后。它记录源码实现，不证明安装配置、实际UID、Root权限、蓝牙密码或设备运行已验收。用户已授权wnfhsj88接手原A任务；本包不改变Core/policy/wire/Root设计或现有十个scope编号。

## 入口与实际约束

| 入口/源码符号 | 调用者与scope | 身份/线路绑定、期限和输入上限 | 撤销/失败与边界 |
|---|---|---|---|
| Edge静态GET；linux.rs boundary/router | 公开静态读取，Host须属于配置的精确origin集合；不以Host授予业务scope | 单Host；静态MIME/CSP/no-store/nosniff；TLS连接握手3秒，HTTP头3秒/32头/16KiB缓存，连接90秒 | 任意Host/不匹配origin拒绝；下载JS不代表业务授权。真实HTTPS证书/地址仍需主线核验 |
| POST /api/v2；api/forward | 已有Edge进程转发，Core逐命令授权；非调用者传入的uid | protocol2、唯一请求ID、MAX_FRAME65536；API信号量8；核心socket/父目录owner及mode、真实peer_cred UID；forward15秒且核对回复ID/版本 | 普通HTTP明确拒绝IssueMedia/CheckMediaLease/CheckMediaPreflight；丢回复不证明未执行，原操作ID须保留 |
| POST /api/network/v1/test；network_test::api/auth_snapshot | token经Core NetworkDiagnostics核验network.manage；新限额写入另需recent_owner | 固定测试目标/有限profile、选定已连接接口和明确蜂窝许可，不接受任意URL；单测试并发/15秒间隔与15秒总预算 | 不自动启APN或静默换出口；本次审计未执行网络测试，不将只读状态当互联网证明 |
| POST /api/media/v1；media::api | 有效正常会话，Core CheckMediaPreflight或CheckMediaLease；实际连接的本地/对端IP需为允许私有地址 | protocol1、规范token/session、worker frame24576/SDP16384，slot等待1500ms；绑定原会话、实际DTLS peer key和CallLease | 取消/断开要清理自己的worker；没有真实peer或租约不可从HTTP声明取得媒体能力；真实听音未验 |
| GET /api/media/capabilities；media::capabilities | 公开有限能力开关，仍经Host边界 | 查询串拒绝，无媒体token/凭据输出 | enabled不是已连接、已授权、可听音或业务在线证明 |
| POST /v1/session；legacy_session | legacy_compat显式开启、单一X-RSH-Legacy-Compat:1；Owner先绑定批准的NAS通道 | 精确JSON、无query、有限会话；兼容信号量2；不将普通browser payload转换为Owner | 未启用默认拒绝，NAS只读语义不获得新写scope；具体旧客户端/网络还待主线实测 |
| POST /compat/v1/read；legacy_read | compat controller与现有NAS会话/批准读取权限 | 有界请求及来源/线路上下文，由compat/Auth处理 | 不进入短信/通话提交器；兼容读取不能授予媒体或网络管理 |
| 首配HTTP /api/v2；captive::allowed/api/boundary | 仅固定热点地址、origin与首配/login/AP有限子集；Core仍逐命令授权 | 固定192.168.100.1:8080，body16384；Root只有开放首配热点期间转发80；没有短信/电话/转发/history/pairing任意入口 | 普通鉴权边界未被页面白名单替代；开放热点/首配认领依原现场门禁，不据源码认定物理信任已验 |
| Core主Unix IPC；authenticated::serve/handle | 正常模式只接预期Edge UID；转发管理模式Access::route分别识别Root/独立worker/Edge真实SO_PEERCRED | 单帧65536；读/写各5秒；并发8；Core实例/当前principal及scope进入实际派发 | 未知UID拒绝；不信payload的uid/pid/authenticated/channel；worker/Root协议与browser命令分流 |
| Core媒体私有IPC；media_ipc::serve/State | 仅配置的实际媒体UID；票据由Core与具体principal/context签发 | 最多2连接，单调connection计数；首帧5秒、后续750ms；media protocol帧2048，绑定boot/line/租约与peer key | ticket单次consume，pulse需原连接且当前context有效，expire/disconnected撤销；不能以蓝牙或IP字段替代票据 |
| 转发Root/worker私有IPC；forwarding/ipc::Access/private_request | Core/Edge/worker必须是不同非Root UID；Root=0、worker=固定UID、browser=固定Edge UID | worker lease30秒及Binding/generation/claim阶段；私有帧解析/读5秒，核验root_only与实际route | browser不能配置worker权限，worker不能当Root；撤销/代次/Owner/realm/SIM由真实Core fence复核 |
| 恢复grant管理；recovery_authorization::dispatch/Auth::{grant,revoke}_recovery_key | feature recovery-grants开启且显式迁移后，recent Owner；client/NAS无权伪装Owner | 独立Ed25519恢复公钥、request_id、expected generation/CAS、有限max_trial_ms；源Auth的同一Owner/realm | feature关闭直接Forbidden；原十scope不重编号；管理grant不是密文验签或可执行恢复会话 |
| BLE库SecurePortV1.receive；auth recovery_capability | 真实BlueZ回调peer与epoch，经注入受信密码/verifier及当前grant才能得到内部能力 | OpaqueBytes和固定分片预算；设备pin、连接nonce/transport nonce、grant/会话绑定必须真实核验 | SecureReply是受信后端结果类型，不会验证密文；当前缺批准密码/生产后端时拒绝，不以bool、MAC、bonding或广播名认领 |
| 转发固定Root status/reconcile；service::Coordinator/request/verified及46规则 | recent Owner通过Core提交；polkit仅允许Core start两个固定unit | Root report只接受同boot/Core/generation/binding、请求后采样、≤10秒且真实PID/无PID；任务8秒预算 | 完成status任务不等于保存成功；status单元User=root但无cap、AF_UNIX、strict/NoNewPrivileges；实际PID1/UID仍需主线核验 |
| Root AP租约消费；channel::RootLease/executor::execute | 受保护Core租约与Binding，非任意shell/AT/目标接口输入 | channel MAX_BYTES8192，boot/realm/request/代次/期限严格校验；恢复/确认走既定事务和guard | 过期/换代/Owner失效须清理原事务；Root真实NM/nft/恢复未在本包运行，不把合成结果当实机回滚 |
| 签名升级/Root采纳；update::admission/private_file/programs | 既有受审签名材料/固定程序与目标，不接受网络调用方给任意执行命令 | 私有文件NOFOLLOW/NONBLOCK、owner/mode/nlink/大小；签名版本/数据/Root契约前置 | 无证据不得admit/confirm，真实Root能力/签名/安装由主线执行，本包不改或调用 |

## Auth与文件写入的共同核对

`Auth::authorize`先核对仍有效的session principal，再比较当前Store中的active/公钥/scopes/generation，最后检验scope位。challenge60秒、会话900秒；`owner`还需clients.manage、Kind::Owner和最近300秒交互登录，续期不自动刷新Owner新鲜度。`network_manager`允许recent Owner或已批准且active/network.manage的Client，NAS被拒；不能将全部NetworkManage简单写成Owner-only。

| 业务组 | 实际scope/额外条件 | 真实入口参考 |
|---|---|---|
| 行状态/网络概况 | status.read | Auth::line_status、core/network.rs |
| 收件/详情/事件 | sms.read；回收站/删除另外要求recent Owner | Auth::inbox/sms_detail/set_sms_deleted/deleted_inbox |
| 短信提交 | sms.send + 实际prepare/submit原ID/线路状态机 | core/sender.rs；不能将展示回执或复制按钮当提交入口 |
| 通话读取/控制 | calls.read / calls.control，真实CallLease/线路与当前principal | core/calls.rs、Auth::{calls,call_detail}，submit与媒体授权分别核验 |
| 网络诊断/限额 | 读取network.manage；SetNetworkPolicy另须recent_owner且同principal | core/network_diagnostics.rs::authorized，写入回执pending不等于APN启用 |
| 客户端/恢复身份管理 | clients.manage + recent Owner；恢复feature/显式迁移另外必需 | Auth::owner、recovery_grants.rs；不存在新Owner或新增scope位 |

Auth/业务Store、AP租约/日志、网络策略与转发状态写入，均应沿现有受限业务/原子存储路径；本表不把HTTP用户名、UI状态或报告布尔值当作文件写授权。秘密扫描器只检查显式公开输入，不能读取这些生产私有文件以“完成审计”。

## 本批仍未知/未验收

未复测任何实际HTTP/Core设备端、真实UID/Root/cgroup/polkit、TLS证书/媒体听音、NM/nft、完整生产BlueZ/GATT/密码互操作、恢复执行或签名安装。此表不是完整SEC-01/02/03/07批准；后续A-28逐状态/撤销/重放矩阵仍需真实接口和额外回归。A-11许可/依赖结果由PR115提供，本包复用其范围说明，不重新生成另一套包版本或宣称已通过依赖审批。

## 固定证据摘要

以下每个文件都与上述完整Git提交逐字节比较后再计算摘要，便于负责人在对应HEAD复查。

| 文件 | SHA256 |
|---|---|
| `crates/edge/src/linux.rs` | `31d2843be5b23257072f9c4e0418efd08dfec64a9c5d5afb1324bd2a8573040b` |
| `crates/edge/src/linux/captive.rs` | `59018bc9b692c2ff3c05fd40bef05988f4b74b14995e8405c7ac2287e201d12b` |
| `crates/edge/src/linux/media.rs` | `0fc70c53355b1b039eeec6713c3ffb3fb4db82f3c0699da5d7de9d0e08d4fffc` |
| `crates/edge/src/linux/network_test.rs` | `05e64439d1de3084ecc507cfaa9d7186d3e34e2d9a01e29d13e34c55d32436c5` |
| `crates/wire/src/auth.rs` | `b05ca749db3b048ebc5635e5b15eeede6bd14c74232220eb11df144ed71cbe50` |
| `crates/auth/src/lib.rs` | `4ddbf623f29def4955f3716c57b38adf13ab2c50917875967c5a42b2d9b884da` |
| `crates/policy/src/lib.rs` | `ee84fae688c1d6e28fec3e44c3f0c94b180b7fe1220ab50e99ee0ad11f84a447` |
| `crates/core/src/authenticated.rs` | `4b726a71a27798f2290264a866647146e123fcdd28ccf02c9acaf86665fee1bb` |
| `crates/core/src/forwarding/ipc.rs` | `4ce858a6c2bb9d498ab71213fc501701824a3ef28e917e578beb7473104c6da1` |
| `crates/core/src/media_ipc.rs` | `61f2318d63eed67a812afe52b8e20cbbeeb1c2f1a18985190bfff3d744e7fbd9` |
| `crates/core/src/recovery_authorization.rs` | `67629867224ca47466713998f6677dbaf721b56bf34171cad7d98d752fa650ab` |
| `crates/core/src/network_diagnostics.rs` | `748d8567e14386ad37e62ed7e52a49be1bd0f1af58b47e9731e1af0c7033c671` |
| `crates/auth/src/recovery_capability.rs` | `95c08dbacbc899209f51907dac8783123c64faf7abf36f587e92d92cea35c9c9` |
| `crates/ble/src/ports.rs` | `680925a697fb5356a2aa430584dee1d6a3e447fdbe1791fbe17ad5fe6e3dd737` |
| `crates/core/src/forwarding/service.rs` | `ef31da96220c551d1df6484b7f56be62391936eca7eaee7139a34741d6fa7210` |
| `crates/setup-ap/src/channel.rs` | `eaf7d1cceab845e2af15cd1ad5274fd29eef8c5fcf6a46bbda6b1993176577b0` |
| `crates/setup-ap/src/executor.rs` | `43661065f9feda07242f12f9bae0e1f400f54ba827a57d0f9f796ba0a4877d9b` |
| `crates/update/src/admission.rs` | `536f1df0028aa9b1f23c4e33d9a4591ae90930feeeed741142e225d1f9385489` |
| `firmware/system/forwarding/rustsimhub-forwarder-status.service` | `0db404d93569b39b8e91886cebcf577cdc88cfe4e0422ba5749eda01d9bb5e39` |
| `firmware/system/forwarding/46-rustsimhub-forwarding.rules` | `2b938272ad7deb8dc9b50fcf135be217560ab38495b548970323e17772787ac7` |
