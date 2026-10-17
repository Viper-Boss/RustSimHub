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


## 2026-10-17：4e6fea6同源整组复验

当前收尾源4e6fea6bf482718e4958d3541a8dee7b362f86c7，归档d02625f65041de8207d8d3d81c20f623c1f9681e00d64abb3f565fee928b6012；209 workspace/六生产ARM、Native21/release3m12s及Node三组通过。Native5516704字节，SHA1733b123fbb40ad18fe4641f181676d943cb5f91359ba76461c096893794bbc2。开发签名三个epoch，同源Core/Media/Edge及不可变update/setup辅助程序已在既有候选RootDirectory重新运行完整上述用例，.private/media-boot-group-4e6fea6.json passed/cleanup_verified=true。

本轮再次证明未初始化不启动/不消耗恢复预算、错误签名在身份生成前拒绝、可信媒体初装/首次owner/严格TLS、实际选中签名槽位的inode/UID/空cap、Media SIGKILL后整组自动回退及身份/收件箱保持、第三版确认/重启保持、第二次Edge SIGKILL后预算耗尽整组停止。台账与payload仍为不同实际文件系统，台账无payload；临时单元/策略/挂载清理、旧四服务active和PCMclosed通过。新拨号/短信/普通蜂窝/原始分区写入均0；不是完整OS启动或已刷入。P6不重复加点，整体仍73.5/100约74%。

为达到既有170MB空闲门槛，先仅回收已结束715旧fixture内27个重复通用程序97730680字节；逐个校验本机完整保留副本/摘要、root555/单链接/设备和inode、无活动进程引用、单元停止/无挂载，再按精确路径非递归移除。身份、短信、日志、manifest和其余状态聚合摘要完全不变，空闲97226752→194990080字节。私有old-media-fixture-reclaim-plan.json含执行记录及restore_sources；715原fixture以后重跑需先恢复程序，不能依赖其旧bin/slot仍存在。新4e6fixture完整保留，勿因旧报告通过而直接再次删除。
