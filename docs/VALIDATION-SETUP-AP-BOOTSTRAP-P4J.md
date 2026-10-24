# P4J 首次认领窗口与有限热点租约

后续[P4K](VALIDATION-SETUP-AP-STATION-TRIAL-P4K.md)已增加STA试连并返回原AP的后端及固定DHCP隔离，尚未接Root循环/Core/网页或保存；下文保留P4J边界。

精确源码1643a52的[GitHub CI40064456827](https://github.com/Viper-Boss/RustSimHub/actions/runs/40064456827)两个任务全部成功，含真实材料CLI步骤；私有完整证据setup-ap-bootstrap-ci-20261024-first.json。候选仍未部署。

2026-10-24。Core已将真实设备的首次LocalSetup窗口接入热点控制面，认领成功后保留原截止时间供配置交接。320项工作区Rust测试、严格Clippy、生产Core检查、三个真实材料CLI联合测试及三程序/库ARM64构建通过。本轮只读确认现有棒子服务正常；没有部署本轮代码、开启热点、切网、拨号、发送短信、启用普通数据或修改证书信任。整体79.5%，上次交付业务185ec6e/confirmed43保持。

这是首次热点租约的源码实现，尚不能完成从热点切换到家庭Wi-Fi的完整流程。现有已认领设备不会自动重新开启首次窗口。

## 具体实现

- `crates/auth/src/setup.rs`：只有实际`LocalSetup::for_device`产生设备窗口证明；绑定实际公钥、realm和随机窗口ID。显式`load`不具备该证明。原300秒、16次预算及持久setup.used保持；签名认领记录实际Owner，后续仅在同一Owner/generation/scopes仍有效且原窗口未过期时提供本地上下文。不序列化或通过网络接收这份证明。
- `crates/core/src/calls.rs`：增加仅内部可用的类型化DeviceIdle任务，前后核验实际窗口、刷新ModemManager通话目录、核对新鲜绑定并要求无活动通话。没有伪造Owner、无匿名业务命令，也不执行拨号或挂断。
- `crates/core/src/setup_ap_control/bootstrap.rs`：独立首次会话记录，与既有Owner热点记录区分。取得Core网络互斥，核验Root材料/TLS准备、实际窗口与新鲜语音空闲，先落盘intent和pending，再发布唯一Start。Root确认前不续发Start或心跳；确认后使用有序KeepAlive，截止时间不超过原窗口，确认失效即取消。
- `crates/core/src/setup_ap_control.rs`、`authenticated.rs`：仅在原显式设备热点服务模式中绑定实际LocalSetup。正常已认领服务模式不增加隐式热点行为。Owner读取/取消仍经过真实登录及network.manage；取消要求最近登录。

## 重启、失败和认领边界

首次窗口不可续开。进程恢复发现pending时，只尝试取消原操作；不会根据磁盘记录重建LocalSetup、重放Start、删除setup.used或重新生成身份。Unknown保留网络互斥，只有与原boot/realm/公钥/材料/Core实例/nonce绑定匹配的Root Closed及回退/隔离释放证明才能关闭会话。Closed证据持久保存并同步目录后才释放互斥。

签名认领成功会关闭匿名认领能力，但允许现有热点在原截止时间内继续工作；它不创建新的300秒窗口。认领发生在热点启动前、预算耗尽、Auth实例变化、窗口失效或服务停止时，均不能借此新开热点。新的Owner会话也不能抢占仍未明确关闭的网络事务。

Root热点执行器、隔离规则及材料协议沿用P4G/P4H/P4I，没有扩大网络监听或硬件接口。

## 实际验证

工作区320项Rust测试通过，比上一阶段增加9项：真实LocalSetup/Auth证明2项、实际p2p D-Bus通话空闲读取1项、首次热点协调6项。覆盖签名认领保留原deadline、错误显式窗口/缺TLS/通话忙拒绝、预算耗尽取消、迟到Ready不复活、错误nonce的Closed不释放、Auth实例替换取消、进程恢复只取消和精确Closed后释放。D-Bus夹具核对拨号/启动/挂断计数均为零。

全工作区全目标严格Clippy、Core无默认开发功能检查通过。真实Core/Edge/Root材料CLI在一次性Linux临时目录运行，3项通过；Root仅为测试父进程，Core/Edge按原非Root角色执行。三程序和库ARM64的ELF machine183、产物散列已核对。这些是未签名开发产物，未嵌入冻结源码包，不能直接作为正式发布包部署。精确提交的CI结论另记。

证据：私有`setup-ap-bootstrap-20261024-first.log/.exit`、`setup-ap-bootstrap-full-20261024-first.log/.exit`、`setup-ap-bootstrap-cli-20261024-first.json`、`setup-ap-bootstrap-arm-20261024-first.json`、`setup-ap-bootstrap-checkpoint-20261024-first.json`。未把前次CI当本轮成功。

本轮只读设备检查：固定SSH身份匹配，Core/Edge/音频路由/网络代理/ModemManager五服务active，无通话，普通gsm连接未激活，源码包散列与已交付43组一致、boot未变。最初使用远端Python的探针因棒子没有python3失败，改用只读shell检查成功，没有为此安装软件。记录`awake-business-readonly-20261024-second.json`，原始设备内容仅在私有目录。

## 尚未完成，下一步

1. 当前新鲜语音空闲证明仍要求SIM绑定；无SIM首配需要明确的无SIM/无活动语音证明，不能简单把“绑定不存在”当作空闲。
2. AP到STA试连接、错误口令/超时回AP、隔离规则及网络互斥的交接；当前Root事务结束会回退，不能直接在其内部调用旧Wi-Fi试连接。
3. 认领后尚未完成联网就重启的配置恢复、已用首次窗口的可信维护入口；不得清除setup.used或重置身份掩盖缺口。
4. 首次配置网页、联合签名安装、实际受限服务和备用维护入口验证，然后安排用户热点/手机配网及冷启动验收。当前不在棒子上开启候选热点。
5. 已部署日常功能可先验收：用户已醒并反馈已部署Wi-Fi“列表和布局正常”，该项只读显示验收通过；未试连接，不能代替热点/切网验收。

PMIC/SMD关机调查继续搁置，累计10010仍44；没有新的短信发送授权。
