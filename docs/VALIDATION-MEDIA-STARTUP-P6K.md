# 正式媒体启动组的候选根实机验收

2026-10-16，源71506f0edfe1383487a33b8adcaf1e0ea78cf9fe，归档bacd57c2f390d398b0ae5f54c872fd248a4d3516ccc50ab572bc6713c069de13。本机202 workspace/20 Native及Clippy、39板级、Node媒体/proof通过；六个生产ARM程序构建退出0。NAS同源Native20项/release退出0（3m11s），5516704字节，SHA b5bebefd8943a8869bc1c36f47b9b8301c34b84a2abe443dd7c39e69e8d3bc01；对应私有build.json和完整日志已核验。

`tools/test-boot-service-group.py --media-lan --failure-role media`在真实棒子的既有候选RootDirectory通过，私有报告media-boot-group-first.json passed/cleanup_verified=true。临时改名单元与开发签名三版包使用真实生产Core/Edge/Native；不修改当前开机服务，不拨号/发短信、不写分区。

- 未初始化不启动、不消耗恢复预算；错误签名在生成任何身份前拒绝。
- 可信初装显式媒体配置、Root私有handoff、完整Core首次owner/严格TLS及重复安装拒绝通过。
- Core986/Media984/Edge985实际Root目录、选中签名文件inode和空cap通过。Media只获得固定设备白名单；没有活动电话不打开PCM。
- 真实Media SIGKILL触发完整Core/Media/Edge自动回退同一factory版本；owner身份与收件箱保持。第三版显式健康确认并重启保持。
- 第二次实际Edge SIGKILL后恢复预算耗尽，整组停止且Media不再存活；没有循环恢复。tmpfs台账没有payload，payload目录为不同文件系统。
- 临时单元/规则和挂载清理通过，旧nassimhub-agent/ModemManager/NetworkManager/rmtfs active、PCMclosed；新增通信和蜂窝测试0。

此前四个已结束P3G fixture的重复通用程序经无进程引用、Root/模式/链接数/摘要及本机保留制品核验后回收，空闲由142716928增至200249344字节。只移除其bin内三个可重传程序，所有状态、日志、失败证据、备份和候选根保留；私有web-generic-reclamation.json记录。不要再假定这些旧fixture的bin仍存在。

这不是完整系统上电、真实NSHSTATE初始化、刷写/回刷或自动更新健康确认。内核精确来源和恢复门禁不变。固定进度P6从6到6.5，总体73/100；P3不加分，浏览器连续声音仍失败。

另一次同源715浏览器诊断新增实际10010一通，累计25：用户报告约4—5秒后无声；Native明确`worker_rtc_failure stage=service kind=Codec`， captured/sent251、received259、played252、dropped5、xrun1，之后独立30秒保护挂断。具体包形状/时长/序号原因尚未确定，不将本启动组通过当作音频问题解决；证据browser-preview-71506f0-diagnostic-first.json和current-native.log。后续源码细分拒绝原因，并修正已独立发现的晚到/重复包不应导致整路媒体终止：仍在Opus前丢弃，不播放、不刷新接收存活时间，其他拒绝和超时保持。参考[RTP RFC3550的乱序/重复处理](https://www.rfc-editor.org/rfc/rfc3550.html#appendix-A.1)；是否命中本次现场原因仍需新ARM实测。
