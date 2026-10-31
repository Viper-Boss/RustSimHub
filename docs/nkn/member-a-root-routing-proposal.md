# A-25/A-26：固定Root监督与远程路由接口提案（待审）

源码基线80db24e9b101dd3fb4b8c951aa7f1e0e1858e65d。对应任务书T6的NknRootPortV1及A-25/26；没有实现或批准新的Root IPC/凭据格式，不安装/启用服务。本包是具体审核材料，不能将自己的进程元数据当成Root授权。

## A-25：保持三个已冻结方法，不创建通用执行器

Rust监督库候选只接`status()`、`start(fixedPolicyHandle,leaseProof)`、`stop(leaseProof)`。PolicyHandle及LeaseProof在库中为Root provider的不可Clone关联类型；**不**在本PR定义成能由网页JSON构造的字符串/布尔。注册、序列化、签名/凭据来源由主线Root实现精确审批，库不打开生产密钥或挑任意路径/UID/网卡。

固定policy至少绑定经核验的选中应用来源/引擎hash、实际低权账户、唯一受管unit/cgroup、允许网络路径及出口规则；实际数值和安装写入表尚未冻结，不能自行占用已有Edge/Core/媒体UID。引擎进程capability/ambient应为空，无TUN/NET_ADMIN/任意LAN代理；Root辅助端只执行预登记动作，并从实际SO_PEERCRED核验调用者及有效租约。NKN SDK本身有动态拨号/重连，必须明确固定出口如何约束其dialer/namespace，不能只给它一个“Wi-Fi已连”布尔后允许默认蜂窝回退。

监督状态候选：Disabled→RequestingRoot→StartingEngine→Ready→Stopping→WaitingExit→WaitingRootRelease→Disabled。任何部分启动/未知ACK/超时进入UnknownCleanupOnly，保留原代次/证明和精确资源，不再start。Ready须有真实backend就绪证据，不能由子进程存活或stdio响应推断；PR128未链接SDK，其unavailable不能变成Ready。

以三个固定owner种类ManualEngine/Remote/Statistics分别持有不可Clone引用；仅释放自己的引用，尚有他人引用时不停止共享引擎。不会因修改转发/统计开关更改普通APN或其他功能开关。重复/旧引用、跨代次释放拒绝，引用数量有限；不是任意调用者提供owner名称就能获权。

每次真实启动总预算拟10秒且不得超出Root原租约；停止总预算拟5秒，子进程退出与Root释放各自必须在同一剩余预算中完成，不逐阶段延长。具体预算尚待审，不能覆盖Root既有更短限制。晚到启动成功只能清理原资源，不能重新挂接为当前Ready；错源/旧代次状态不覆盖当前视图。Root stop结果不明确时只查询原操作/资源，不发新start或停另一个代次。没有Root可验证的退出/释放证明就保持Unknown；库不自行调用shell、systemctl或杀任意PID。

自动重连候选仅在当前同一授权/epoch仍有效、确实未产生不确定部分启动时允许：单一窗口内最多3次、最小间隔5秒、总窗口30秒，先检查原lease未过期，不重新获得/续期授权。UnknownCleanupOnly不适用自动重连，必须查询原资源状态并显式重新取得有效授权。不通过增加租约/无限循环掩盖退出失败。

负责人需先确定：固定policy/账户/出口约束与准确来源；Root证明/状态新鲜度的真正可信producer；精确序列化/错误分类/原操作查询语义；以上预算及重连条件。获审后实际Rust监督代码用Mock Root/引擎端口验证部分启动、停止超时、旧epoch晚到、资源引用与异常退出，真实Root/cgroup/UID由主线有限核验。当前未用Mock装成生产后端。

## A-26：成员/传输不授予业务权限

固定参考nknguard@c32952d7586676fec10f71a30b22969cb24881f3的pkg/membership/membership.go、pkg/protocol与发现/信令/媒体源码只读来源见PR115 manifest。原网络成员证明只能证明其自身网络成员条件，不能变成RustSimHub Owner/RecoveryGrant；不将统计公钥、NKN地址或连接成功当注册设备/安装数/人。

候选只开放两个版本化服务ID：`core-auth-v2`（正常Core认证命令/查询）与`call-media-v1`（现有媒体预授权/票据绑定通道）。不接受host/port/URL/socket路径/CIDR/接口名称或通用字节转发到任意LAN。实际解析分别复用现有wire AuthRequest及media Permit/Ticket，不建立第二套发送/拨号/租约队列。服务目录及编码需主线/D共同审核，尚未上线。

普通远程会话需设备pin、独立传输密钥及正常已注册客户端的真实证明；按现有Principal/realm/Owner generation/客户端撤销核验，每条业务请求仍交Core现有授权。membership有效但Core身份缺失→拒绝；scope不足/旧realm/旧SIM/旧会话/过期/重放→拒绝且不触发业务。只维护现有10个Scope/ALL_SCOPES=1023，不增位/重编号、不把Client/Nas升Owner。跨传输绑定必须包括Core实例和其生成的连接nonce，不能用UID/PID/地址等同会话。

BLE恢复与普通NKN控制分属不同purpose/context和入口。BLE恢复只能经过真实RecoveryRequestVerifierV1/RecoveryCapability执行受限动作；不得借`core-auth-v2`把BLE UID接入普通Owner/Root socket。反向也不能用一个恢复grant批准普通NKN短信/通话。媒体仍逐次绑定原peer/call/line/许可及撤销，NKN只送信令，不代替DTLS/授权或提供任意UDP/TCP出口。

防重要求：建立会话前验证版本/purpose/pin/双方密钥/代次；同一会话每方向严格单调sequence，不接受重放/乱序/重复，在已核验上限内计数，耗尽关闭而非归零。授权撤销/目标变更/父连接丢失清理私有上下文，不能让旧会话重连续权。不记录原始消息、成员秘密、端点、SIM/短信或凭据。

主线/D负责真正Core权限桥和生产接线；A提供获审后的协议/有限路由策略库及真实实现的模拟端口失败矩阵。没有真正加密/可信Core producer时返回unavailable，不转发明文或接受payload“已认证”。本提案没有实现正常远程调用，也不作为现有raw AuthCommand新入口。
