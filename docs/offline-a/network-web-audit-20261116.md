# A4离线差距表：配网、网络、灯控和网页

2026-11-16交付后回读main `6e1cd19bc9dd05fd150157c422590c1a801cc0b0`：任务书已更新为热点暂缓、USB电脑工具优先。
下表AP专用测试是按旧任务截面已完成的历史离线证据；热点部分统一延期，不再安排新增扫描/试连/返回/保存或手机验收，
不标完成、不减原分母。后续仅继续共享Wi-Fi事务、USB `network.manage`授权和日常网络等获准范围。

基线`b15ea3f561bf9e2a1268f8df5b6f0973cc83bbbd`。网络网页修复受测tree为
`a00522e061a46ccdfb5479ef5c6e5b077a2e1121`，命令/来源与边界见[交付报告](../deliveries/OFFLINE-A-NETWORK-WEB-20261116.md)。
下列28项逐项记证据，不将离线复验等同现场验收，不改全局看板或原权重。

最新main已正常合入，组合受测tree为`8e353f3fb412874a03e2808779b75b2140515a49`。
新增组合证据是27组非热点Node、Edge71/严格Clippy/fmt、实际主页与控制台三宽及模块404负对照通过；
首次浏览器进程权限启动失败单独保留。未重跑任何热点专项，未把历史241项写成新组合结果。
精确来源、命令和失败见交付报告的“最新main组合复验”。

| ID | 已核实现及本轮证据 | 剩余边界 |
|---|---|---|
| AP-03 | `crates/setup-ap/src/station_commit.rs`及`nm/tests/commit.rs`；180项AP测试含真实私有D-Bus保存/丢失回复/检查点；实际配网页same-ID保存 | 真机最终保存/Root安装不在本轮 |
| AP-04 | `station_trial.rs`、`nm/tests.rs`；错误密码、回退、拒绝/超时、重复请求；实际浏览器wrong-password场景 | 真路由器错密/回热点需现场 |
| AP-07 | 原`guard.rs`、`executor.rs`和有界状态/结果；模拟Owner/NM前置 | 未启真实热点，不称首次设备隔离通过 |
| AP-08 | 实际`setup.html/setup.js/ap-wifi.js`三宽11场景；原NM有限对端与返回/确认状态机 | 浏览器外部API为合成，非真实AP/STA切网 |
| AP-09 | AP模块无NAS接口前置，私有NM/Owner测试不连接NAS | Rootfs/首次启动材料由B，USB仅供电冷启动留主线 |
| AP-10 | `nm/tests/scan.rs`、`ap_rescan/tests.rs`；有限SSID/候选、空扫描、错对象/过多结果/改代拒绝；浏览器刷新 | 真实隐藏SSID/重命名网络验收未做 |
| AP-11 | 原事务/commit持久状态与恢复反例；浏览器同ID重开/保存、不自动补发 | 断电/设备启动不模拟为已完成 |
| AP-12 | `tools/test-web-diagnostics.mjs`、实际setup错误/恢复提示、一键入口 | B补独立设备手册和实际首次配网记录 |
| NET-01 | `crates/network-agent/src/accounting_tests.rs`：所有重置日/短月/跨年/回拨/重开 | 真实月结/时钟变化未操作 |
| NET-02 | `linux_tests.rs`有界真实文件解析；计量源缺失、u64、方向溢出、SIM/boot代次；网页未知/图表缺口 | 未改主机nft/路由/DNS或真接口 |
| NET-03 | `web/network.js`、原Node渲染用例；零值/缺值/有限速率、信道推算标注 | 不编造信号或带宽；实际无线读数待设备 |
| NET-04 | 原quota enabled/uncertain拒绝模型；403后实际按钮禁用；注销清除蜂窝测试勾选 | 没启蜂窝/APN、没做流量测试；真限额/漫游需主线授权 |
| NET-05 | 原采样boot/SIM/计数器重置与原持久账目测试 | 真切换/掉电/速率未验收 |
| NET-06 | 页面注销清理测试状态/同意，所有本轮网络测试仅本机合成 | 原卡恢复/普通数据实际关闭仅主线现场核对，本轮不操作 |
| LED-01 | `crates/status-led/tests/offline_policy.rs`固定网络颜色组合，实际库测试 | 不读写GPIO，不证明硬件颜色/电平 |
| LED-02 | 同文件dominant modes优先级、故障和未知；私有bus测试替代PID1/NM | 真实红灯故障未触发 |
| LED-03 | IO/固件提示固定稀疏预算与限频 | 不做硬件闪烁/功耗结论 |
| LED-04 | lease边界、过期/退出、配置提示不会永久保持；原Node indicator用例 | 真设备白灯/租约到期留现场 |
| LED-05 | 通知组合、确认清理及持久标记的原模型 | 未发真实短信/未接电话，不修改通信存储 |
| LED-06 | staged记录畸形/无证明不能宣称更新可用或运行；原可信状态入口 | 不接管B更新器，不执行升级/硬件紫灯 |
| LED-07 | `linux_input_boundary_tests.rs`仅临时文件/FIFO/权限；私有P2P bus及快照有效期 | 未运行生产灯循环或安装服务 |
| WEB-01 | 原一键test-center/console/session等Node套件与实际主页装配 | 默认日常页与测试显式展开debug截图区分；手机外部验收保留 |
| WEB-02 | 原operation-receipt/event-sync/configuration-state；网络旧会话finally隔离；后续原ID先落盘/读回、真实IndexedDB刷新、CAS/Unknown原记录已补，见[限额报告](../deliveries/OFFLINE-A-NETWORK-POLICY-20261116.md) | 原API只有最后请求ID，历史被覆盖且无匹配证据时保留Unknown；不同浏览器不宣称服务端CAS，实机/掉电仍未验收 |
| WEB-03 | Edge真实渲染320/390/1280，长中文控制台/短信列表、overflow断言和截图 | 不是用户手机/全功能视觉验收 |
| WEB-04 | 真实IndexedDB钥匙、原Ed25519签名登录/重用及注销；原clients/NAS Node用例 | 全部合成身份，不读取用户钥匙或安装设备证书 |
| WEB-05 | 实际403、注销/迟到、旧读请求/测试忙槽；25项Node suite及原一键错误反例 | 没有实际拨号/发短信/互联网探测 |
| WEB-06 | 网络权限失败清旧状态、按钮禁用；保存配置与运行状态原组件区分 | 保存不等于设备已运行，B/设备组合未验收 |
| WEB-07 | 原HTML/ESM/CSP/IDB实际主页、控制台/配网页；缺NKN模块真实404；六个VM定点缺导入反例 | 修复旧负对照被无关新import抢先失败的假设，保留原失败；外部手机/公网不可由本机判通过 |

AP/计量/LED共241项Rust在基线通过；这些Rust路径与最终tree逐字节一致。它们是已有实现的本轮复验，非新写241项功能。
Edge/Node/浏览器最终结果单独记录，不把默认/不同feature或重复执行累加成独立总数。
本表与PR258的20项FWD/NAS表共同提供48项更具体的审计入口；其中仍有未验证及外部部分，**不是48项已完成**。
其余45项（SMS/通话、NKN/统计和远程）仍需逐条补证据；A1新Root接口等待负责人审核，不能据已有查询功能称上报已实现。
整体已验收比例保持 **81.5%**。
