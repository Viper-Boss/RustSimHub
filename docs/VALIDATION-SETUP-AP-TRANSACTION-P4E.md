# P4E 受保护配网热点事务层

2026-10-24。新增Rust库`crates/setup-ap`，实现临时AP的真实NetworkManager后端、Root固定nft隔离和有界事务。它尚未接入Core授权/首次认领、Edge证书与Origin、网页或开机服务，也没有部署或开启棒子热点。上轮确认的业务仍为185ec6e/confirmed43；整体79.5%，不能据此增加独立配网验收分数。

## 已实现

每设备生成独立CSPRNG材料：随机SSID后缀、192位随机口令（48个十六进制字符）和独立UUID，不从IMEI、MAC、SIM或通用默认密码推导。凭据类型不实现Debug/Serialize，口令内存使用Zeroizing。可信交付接口单独命名，后续只能用于可信USB/控制台或每设备标签；本轮不提供公开网页读取口令接口，也未生成真实设备标签。

AP固定为wlan0、2.4GHz、WPA2-personal/RSN/CCMP和客户端隔离，IPv4使用172.31.85.1/24的NM shared模式提供地址，IPv6禁用，无默认路由、autoconnect=false。候选配置采用volatile和dbus-client，不写入持久热点配置。设备/路径/地址/端口/加密字典不接受网络客户端覆盖。

前置只读检查固定NM唯一owner、最多16个设备、一个被管理的wlan0、AP能力及无线/网络已启用。只支持未连接、没有保存Wi-Fi的首次配网起点；已经连接的路由器Wi-Fi直接拒绝，不会为开热点断开它。检查其他接口地址与AP子网冲突，最多64份连接设置；拒绝普通蜂窝自动连接配置，不更改radio、IMS或APN。AP/STA并行和已有Wi-Fi丢失后的恢复入口仍未实现。

事务最多300秒，独立NM checkpoint最多330秒，调用方取消/消失、授权检查失败、热点失去就绪或到期均进入清理。真实后端只允许一次checkpoint请求和一次激活；激活已生效但回复丢失时，仍按精确UUID/SSID/接口/模式核对并清理自己的临时配置。checkpoint创建回复丢失时不重发，不根据路径列表猜测或销毁别人的checkpoint；结果保持不确定，等NM自身计时或后续显式核对。

先对未连接状态作只读预检，再安装转发隔离；隔离核验后才创建checkpoint、激活热点。Gate在各步骤及活动期间重新核对。回退只处理自己的checkpoint和候选，验证无线恢复未连接、精确UUID消失后才移除隔离；最多10秒清理和5秒隔离释放。无法证明时返回Unknown并保留隔离，不自动重开、重试、延长窗口或保存热点。

## Root隔离

NftGuard只执行固定/usr/sbin/nft参数，无shell、flush-ruleset、自由接口或命令参数入口。只拥有inet rustsimhub_setup表，不修改流量计量/额度表。

- AP接口只接受IPv4 DHCP客户端到67端口、同AP子网到172.31.85.1:8443的HTTPS。
- 其余AP输入、IPv6、DNS转发、SSH及其他本机服务拒绝；双向转发均拒绝，不提供NAS或蜂窝流量共享。
- create table采用排他语义，已有同名表时拒绝，不能在未知结果后覆盖旧隔离。
- 新装后及活动期间对照固定规则的完整内核JSON投影，忽略的只有内核handle和版本metainfo。链策略、优先级、端口、目标地址或表达式变更都会失败，不以“表存在”代替隔离生效。
- Root权限仅属于此固定隔离后端；没有为Core/Edge增加NET_ADMIN。本轮尚未安装Root启动器或服务。

172.31.85.1必须加入后续设备证书SAN及精确HTTPS Origin/监听配置。当前证书/网页不能假装已经支持该入口；不通过关闭TLS检查或放宽任意Origin完成配网。

## 验证与实际边界

276项完整工作区测试、严格Clippy、Core生产功能检查和ARM64库构建已通过；库归档内两个ELF成员核对为AArch64。精确源码CI结果另记，不能用本地结果代替。最新私有索引setup-ap-library-checkpoint-20261024-first.json。新增真实p2p D-Bus夹具覆盖临时激活/清理、激活回复丢失、checkpoint回复丢失、NM自动消费checkpoint、当前连接拒绝和owner变更；事务夹具覆盖取消/调用方消失、各阶段权限失效、隔离失败、激活失败、回退未知后保留隔离。

NAS上另在独立网络命名空间加载了生产固定规则：初始仅loopback；加载成功，重复create被拒绝且旧规则逐字保持，只删除该私有空间的精确表。NAS主机防火墙与棒子无线未修改，没有测试报文或真实热点。私有报告setup-ap-kernel-isolation-20261024-second.json保存结果和规则哈希；第一份因sysfs挂载反映宿主视图而在执行nft前拒绝，改用当前命名空间的if_nameindex核对后通过，失败报告保留。

第一轮测试通过但脚本末行CR影响了-D warnings，不能作为严格Clippy通过；最终LF脚本及完整工作区检查另存，不抹去前一份记录。公开setup-guard-v1.json仅包含本项目固定规则在隔离内核的表示，无设备标识/SSID/口令。

## 接下来必须接上的功能

1. Root协调器和Core有限维护租约：Gate必须来自真实Owner最近登录，或明确的首次设置窗口/所有权证明；持有Core网络互斥和无活动通话条件。不能把测试中始终允许的Gate接到产品。开始意图和Unknown须持久保存，服务重启不能重放激活。
2. 每设备可信材料、证书SAN和精确Origin/监听配置，以及首次认领窗口/已消费标记；先交付口令与设备指纹，不能让陌生AP访客先占有设备。
3. AP到路由器Wi-Fi的显式试连接/确认/回退。当前已有配网要求USB，不能直接删除该条件。需要协调热点原checkpoint、临时候选和HTTPS可达性，失败时恢复自己的AP供用户重试，成功保存后停AP；不能让本事务的正常结束回退已确认的路由器连接。
4. 切网时隔离规则随已证明的STA地址调整，并在AP确实关闭后释放；当前固定AP输入规则不能直接用于STA确认，否则会阻断新地址。设备发现与稳定访问地址另验证。
5. 真实AP/DHCP、手机连接/浏览器、无NAS认领、错密码/超时、服务退出、保存后冷启动及转发报文矩阵。

用户休息期间只做源码、构建和隔离验证，不开启设备热点、不切换SSID、不拨新电话、不发短信、不启用普通数据，也不恢复PMIC/关机试验。

## 文件与依据

- crates/setup-ap/src/lib.rs：随机材料与有限固定值。
- transaction.rs：窗口、授权Gate、取消、清理及Unknown。
- nm.rs：系统D-Bus唯一owner、有限设置、checkpoint和精确候选清理。
- guard.rs及setup-guard-v1.json：Root固定表和逐条规则核验。
- 官方：[NM shared IPv4](https://networkmanager.dev/docs/api/latest/settings-ipv4.html)、[AP及客户端隔离](https://networkmanager.dev/docs/api/latest/settings-802-11-wireless.html)、[WPA设置](https://networkmanager.dev/docs/api/latest/settings-802-11-wireless-security.html)、[checkpoint及临时连接](https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.html)、[nft排他create语义](https://www.netfilter.org/projects/nftables/manpage.html)。
