# A25/A26：实际 Root、Core 与共享 SDK 提供者

本包承接实际受管 SDK 候选及 PR178，不改已合并提交、不改全局进度、不安装部署。
实现、固定路径/新协议/策略/角色/预算和未启用边界见
[Root 候选说明](../nkn/managed-root-candidate.md)。用户已明确转交隔离代码实现，
负责人仍审核与决定生产采纳。

真实 Root 程序核验选中程序/来源、独立身份、实际 NSS、原租约、新鲜固定网络、
实际 nft、单独 cgroup、实际低权进程与退出。Core 通过真实 Root/Engine ports
复用原三引用监督器，并接固定普通 NKN listener、真实 Root 来源证明、PR178
原 Auth/Noise/Core/media 业务路径。关闭 Remote 的父 fence 立即永久失效，保留
其他拥有者；重开不恢复旧来源。NoNewPrivileges 核验覆盖全部 Go 线程，不能靠
某个 goroutine 的单线程 prctl。所有候选功能默认关闭。

已实际执行：真实低权 Rust Auth→原监督器→Go Root→生产 SDK→自有 TLS/WS 节点；
三引用共用、逐引用关闭/旧 fence 不恢复、最后释放后实际无 PID/cgroup/规则。
另有真实 Root 显式停止、Core 连接 EOF、原租约到期及篡改规则四个场景。内核
探针由 Root 验证五个目标确实可达，再以 NKN UID 证明仅允许固定接口公网 TCP/
固定 DNS，拒绝私网、usb0 和其他 UDP。篡改规则后实际回收 SDK，但保留规则与
Unknown，未伪报释放。普通 suite 中专用 Root UID 入口明确 ignored，另在命名
空间手动运行，不重复相加为普通 suite 通过数。

证据是自有隔离 cgroup/netns 与真实内核，不是设备/systemd/NetworkManager 验收。
合成地址只在 network=none 命名空间；无公网 NKN/交易/真实用户业务或消息。最终
冻结 HEAD、全套验证、哈希和 PR 回读会补充；早期工作树结果不冒充最终制品。

后续仍推进完整 candidate Core provider/网页装配、A22 Root 采集映射、A29
queue4/worker5/七阶段 SMTP，以及 BlueZ/WASM/CSP/PWA 和 A30。具体 review/dependency
审批、真实角色安装、硬件/ARM 运行和生产部署不由本包代签。整体验收81.5%、20/169。
