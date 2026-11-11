# 整体剩余任务盘点 · 2026-11-11

任务8正式入口、连续守卫及已支持组合离线闭环已由PR215固定报告交付并按指示批准，尚未合入main；ARM/QEMU不等于棒子实测。通用刷机包221及安装记录218仍是本人PR，平台不允许自批。原验收81.5%，还差18.5点；完整台账20/169、尚有149个收尾动作，不是149项待写代码。

## 已交付、批准与来源

- PR215：任务8离线生产接线已交，最终代码8470608/完整HEAD610d7fa；作者报告workspace默认1466、全特性1669与正式ARM完整链通过，本轮未重跑。已按用户指令批准，未合并、未设备验收。
- PR216：统计时钟回退修复已交，作者报告50项Windows测试；已直接批准，未合并。
- 旧195/196/199/200也已按指示批准；200的代码历史已随206进入main，PR本身仍指向旧组件base；不能按开放PR数当未开发功能。
- PR221：通用刷机包/首启固件提取与身份生成离线交付；未真机刷写，首次配置只走USB，独立热点尚未装入该通用包。
- PR218：作者记录应用72→73→74安装，11-07真机浏览器登录/只读接口成功；非本轮设备探测，仍有短信排序/时间、通话核对中、127/128计数差待查。
- 当前main55f9783已含217/219/220控制台与登录修复；不能把215的未合并产品与该设备版本混成同一发行。

证据（固定HEAD，本轮只读核对，作者测试不冒称重跑）：

- https://github.com/Viper-Boss/RustSimHub/blob/610d7fa63b1a0f129a3f146c0595171868f8e059/docs/deliveries/web4-production-wiring-20261104.md
- https://github.com/Viper-Boss/RustSimHub/blob/ee379410ef81ba4102504db6e7425e4438e21d37/docs/VALIDATION-APP72-CONSOLE-INSTALL-20261107.md
- https://github.com/Viper-Boss/RustSimHub/blob/1e3de57b6864aa44d1ba9fe1a0db342efe075663/docs/VALIDATION-GENERIC-FLASH-PACK-20261107.md

## 剩余八个收尾方向

1. 主线接纳与组合：215、216及旧195/196/199/200等当前仅批准；221/218本人PR需其他协作者批准。接纳后必须按新的完整HEAD冻结，保留当前报告的支持边界。
2. 完整发行与第二设备：把任务8产品组合与通用镜像统一来源；核对匹配boot/DTB/modules/固件、锁定系统材料、真实PID1/沙箱，完成首次刷写/回读/冷启动/恢复。旧74镜像不等于新产品固件。
3. 首配与网页实际缺口：通用包首次热点集成仍需设计/接线；真实AP→STA/错密/返回热点/最终保存复验；控制台短信排序/时间解析、通话状态及计数差需定位和修复。
4. 产品支持矩阵：product-recovery-health-v1仅forwarding/schema9/worker4，v1还要求相同Core字节基线/候选/回退；AP/SMTP5自动确认明确不支持，EmailRecovery完整链和WASM未验证。扩展支持需要开发；若首版按有限矩阵发布，准确保留不支持声明。
5. 电话/短信/网络/NAS：多客户端、撤权、长通话/DTMF、换卡/MM换代、真实流量限额、NAS授权控制及双向声音等剩余真实边界回归；不重新派发已完成模块。
6. 转发/NKN/统计/蓝牙：真实递送及关闭冷启动、外网/中继/ON worker、获准统计网络和BLE HCI/RF/手机互操作；资源/账户/外部服务及独立授权到位后实测。
7. 更新/迁移/恢复故障：已交隔离安装、迁移和限时健康证据需落到实际设备；验证真实SIGKILL/写满/断电、回退兼容、保留后写数据及整机恢复，不能用合成PID1替代。
8. 发行终验：安全/依赖许可证与公开产物扫描、性能/功耗/组合长稳、第二棒子验收、用户及恢复手册、最终冻结与逐项签认。PMIC搁置，手机原生App不在首版。

## 原台账逐模块未验收数量

这些是整项验收未闭环，不是未写代码计数。原状态/权重/分母未变；不按PR数或测试数制造开发完成率。

| 模块 | 未闭环动作 | 稳定ID |
|---|---:|---|
| FWD · 短信转发 · 独立服务 | 12 | FWD-06, FWD-08, FWD-09, FWD-13, FWD-14, FWD-16, FWD-17, FWD-18, FWD-19, FWD-20, FWD-21, FWD-22 |
| AP · 独立首次配网 · AP→STA | 8 | AP-03, AP-04, AP-07, AP-08, AP-09, AP-10, AP-11, AP-12 |
| SMS · 短信 · 同步与故障收尾 | 8 | SMS-01, SMS-02, SMS-03, SMS-04, SMS-06, SMS-07, SMS-08, SMS-09 |
| CALL · 电话与双向音频 | 10 | CALL-01, CALL-02, CALL-03, CALL-04, CALL-05, CALL-06, CALL-07, CALL-08, CALL-09, CALL-10 |
| NET · 网络诊断与月流量边界 | 6 | NET-01, NET-02, NET-03, NET-04, NET-05, NET-06 |
| REMOTE · 远程HTTPS、信令与TURN | 7 | REMOTE-01, REMOTE-02, REMOTE-03, REMOTE-04, REMOTE-05, REMOTE-06, REMOTE-07 |
| NAS · NasSimHub兼容与迁移 | 8 | NAS-01, NAS-02, NAS-03, NAS-04, NAS-05, NAS-06, NAS-07, NAS-08 |
| UPD · 签名应用、整机组合与更新 | 8 | UPD-02, UPD-03, UPD-05, UPD-06, UPD-07, UPD-08, UPD-09, UPD-10 |
| REC · 备份与完整恢复闭环 | 3 | REC-03, REC-04, REC-05 |
| LED · 状态灯真实事件联动 | 7 | LED-01, LED-02, LED-03, LED-04, LED-05, LED-06, LED-07 |
| WEB · 日常网页与一键测试收尾 | 7 | WEB-01, WEB-02, WEB-03, WEB-04, WEB-05, WEB-06, WEB-07 |
| STAB · 性能与长期稳定 | 5 | STAB-01, STAB-02, STAB-04, STAB-05, STAB-06 |
| SEC · 权限、接口与依赖审查 | 7 | SEC-01, SEC-02, SEC-03, SEC-04, SEC-05, SEC-06, SEC-07 |
| REL · 第二台与正式发布验收 | 6 | REL-01, REL-02, REL-03, REL-04, REL-05, REL-06 |
| NKN · NKNGuard原生远程接入 | 11 | NKN-01, NKN-02, NKN-03, NKN-04, NKN-05, NKN-06, NKN-07, NKN-08, NKN-09, NKN-10, NKN-11 |
| STAT · NKN近期活跃安装统计 | 9 | STAT-01, STAT-02, STAT-03, STAT-04, STAT-05, STAT-06, STAT-07, STAT-08, STAT-09 |
| BLE0 · BLE · 板级可行性 | 4 | BLE0-01, BLE0-02, BLE0-03, BLE0-04 |
| BLE1 · BLE · 加密GATT协议 | 4 | BLE1-01, BLE1-02, BLE1-03, BLE1-04 |
| BLE2 · BLE · Core恢复授权 | 4 | BLE2-01, BLE2-02, BLE2-03, BLE2-04 |
| BLE3 · BLE · 低权限设备服务 | 4 | BLE3-01, BLE3-02, BLE3-03, BLE3-04 |
| BLE4 · BLE · 网页与离线PWA | 4 | BLE4-01, BLE4-02, BLE4-03, BLE4-04 |
| BLE5 · BLE · 配置器公开发布 | 3 | BLE5-01, BLE5-02, BLE5-03 |
| BLE6 · BLE · 真机互操作与恢复 | 4 | BLE6-01, BLE6-02, BLE6-03, BLE6-04 |

历史剩余收尾工时176.5–420小时尚未按新交付重估，含验证与发行，不可当纯编码工时或承诺日期。详细动作见[完整清单](progress/remaining.md)。本轮未合并PR、不编译产品、不签名、不访问设备。
