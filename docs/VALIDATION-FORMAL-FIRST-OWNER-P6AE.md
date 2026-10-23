# 正式业务首次设置与通话试验

2026-10-23。在已安装的新Linux系统上首次创建正式业务身份、数据库、HTTPS/媒体配置并启动完整应用服务。用户实际完成首个Owner初始化及登录；该正式首启/证书交付补齐P1最后0.5点，P1为15/15，整体77.5%。不为未通过的通话或未做的冷启动加分。

## 已验证

- 用户明确选择复用已测试Wi-Fi，并确认现在参加首次设置；另行批准本次准确指纹证书的Windows当前用户临时信任。导入前核对DER摘要，记录准确证书及是否由本轮新增，本轮测试结束后已核对移除本轮新增项，原有根信任保持。
- 原信任factory epoch16/schema6的官方`rustsimhub-install-initialize`成功，创建稳定身份、业务库、Root身份锚、媒体配置及六文件私有交付，最后写installation.complete。未自行替换维护密钥或重造发布根。
- 初始化完成后先核对服务仍停止、setup.used不存在；用户就绪后只启动一次正式应用target。select、Core、Edge、Media均active，身份Unix套接字确认UID986及交付公钥，窗口实际打开。
- 用户确认“已初始化并登录”；随后固定身份SSH读取窗口已关闭，诊断快照确认只有一个principal且唯一active Owner。未读取或复制浏览器私钥。
- 本机严格校验证书与主机名的Wi-Fi HTTPS访问、准确已交付app.js字节、媒体能力与设备公钥匹配通过。first探针误用了CLI字段名而失败，second按真实public_key数组核对通过；未放宽TLS。
- Wi-Fi/USB均connected、普通gsm disconnected，Core/Edge/Media保持运行。没有重启、刷分区或激活普通数据。

## 电话尚未通过

本次持久库和ModemManager日志确认两笔10010拨号：第一笔拨号与挂断均submitted，但呼叫一直dialing至挂断；第二笔为unknown/permitted，ModemManager报告QMI错误90/IncompatibleState，期间注册home→searching→home。两个呼叫最终都是ended/observed。

只读监测窗口覆盖第一笔，观察到呼叫创建和终止，PCM始终关闭；其passed只表示该监测的终止/清理检查通过，**不表示通话声音通过或只有一笔拨号**。用户最后报告“拨号结果未确认；不会重拨”。没有新增短信或录音。累计10010从38更新为40；不能用短监测的单笔数覆盖持久记录的两笔。

没有证明网络注册变化、IMS、CS回落或其他原因中的哪项导致失败；不能把QMI错误直接当成Rust/音频实现错误，也不能声称没有软件问题。未自行更改网络制式、重启基带或恢复此前搁置的PMIC/SMD调查。

另发现一键流程在返回unknown但带匹配租约时先报错、丢掉租约，不能立即尝试有界挂断；修正及手动归档入口见[P3J](VALIDATION-UNCERTAIN-CALL-CLEANUP-P3J.md)，尚未部署。

## 证据与接手

- `.private/formal-initialization-20261023-first.json`：官方首次准备成功、私有交付位置。
- `.private/formal-start-20261023-first.json`：一次正式启动成功，记录准确PID和最初窗口开放。
- `.private/firstboot-certificate-trust-20261023-first.json`：准确临时信任及后续清理状态。
- `.private/formal-https-20261023-second.json`：严格HTTPS及设备pin验证。
- `.private/formal-call-20261023-first.json`：第一笔只读监测/PCM关闭；不是完整电话验收。
- `.private/formal-call-inspection-20261023-first.json`：两个呼叫、操作阶段和基带日志；诊断库为在线文件两轮字节保持的只读复制，本机SQLite quick_check通过，不能当成正式一致性恢复备份。设备无Python/SQLite CLI，最初只读Python检查失败保留。

设备实际业务仍为9528712/epoch16，不是随后离线67ba519候选。正式身份已创建且Owner已认领，**不能重跑初始化器、删setup.used或另造Owner**。后续须走已初始化设备的签名应用升级与受控重启路径；此前仅接受空正式状态的交付helper不再适用。下一步交付P3J并验证异常操作查询/归档，再按实际网络证据安排语音复测和独立冷启动。

收尾只读健康复核见`.private/formal-final-health-20261023-first.json`；只有其passed=true才能沿用“当前服务/注册/PCM和数据关闭正常”的结论。P3J交付后还需重新核对，不能把旧PID和来源套到新程序。
