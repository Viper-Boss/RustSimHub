# U4 · 实际Root配置与持锁数据观察

2026-10-27，UPD-01阶段四。新增有限只读观察，不是完整更新准入；既有stage/choose行为尚未由本模块授权，UPD-01继续进行。

## 实现与边界

生产CLI `rustsimhub-update inspect-maintenance`仅Root可用，固定读取实际根目录下的已知Core/转发/AP配置、业务库及outbox，不接受任意路径或外部JSON作为实际观察。只有完整且逐字节匹配的已知安装档位才分类为device、forwarding或setup_ap；Root助手只给实际摘要，尚不声称其签名来源已核对。未知Core drop-in、运行时覆盖或不完整材料拒绝。

数据按Core正常启动顺序取得业务与outbox的真实writer锁，检查所有者、权限、链接、节点身份、辅助文件与恢复标记。已有WAL/SHM/journal须走独立恢复，观察不清理或迁移；干净WAL头业务库在持锁、无辅助文件条件下通过只读immutable URI读取，路径逐字节编码，查询有有限操作和时间预算。

`MaintenanceSnapshot`持有文件、SQLite连接和writer锁，支持重查Root/目录/文件身份。`compare_versions`从该实际观察构造U3输入，`LiveVersionCheck`借用快照，在比较期间保留锁，拒绝把序列化报告当成仍有效的锁。程序metadata仍需后续绑定确切验签字节，实际systemd加载状态也须单独核对；报告的authorization_verified、effective_systemd_verified和signed_programs_verified均为false。

## 实际验证

固定766文件工作快照SHA-256：`2ad8d3299463197e3c4543860a27421cdd058b0bfd9bb9fc161fe84c03623fc4`。包含明确U4变更覆盖，未混入旧README未提交笔记。源码/依赖只读、UID65534、空能力、NoNewPrivileges、私有网络和设备环境下：

- updater共30项单测通过，其中本模块13项覆盖实际Store/Queue、互斥writer、辅助文件/恢复标记、材料缺失和修改、符号/硬链接/权限、节点变化、无原文回显、未知覆盖、URI特殊字节，以及实际旧outbox迁移后回退读能力。严格Clippy、workspace fmt、真正生产Native更新器构建通过。
- 同源Native更新器再进入单独Root隔离测试；18个真实CLI场景通过，使用实际UID986的数据/锁和非Root调用。覆盖三个安装档位、outbox缺失/空目录、业务/outbox忙、WAL/SHM/真实异常退出journal、部分Root、未知覆盖、损坏数据库、错误所有者、非Root、符号链接、缺锁和FIFO。每次比较完整合成目录的字节/节点/权限/mtime，无修改或辅助文件创建；不输出合成敏感正文。
- 之前Root夹具失败记录保留：地址族限制导致NETLINK不可用，随后显式User=root的临时systemd启动在本机丢失CAP_SETUID。用最小探针分别复现；最终临时测试单元采用默认Root，保留原六项能力上限、NoNewPrivileges、私有网络/设备、只读源码及运行预算。没有更改主机安全设置。夹具使用subprocess明确user/group/extra_groups，并在子进程核对真实身份，避免preexec异常隐藏errno。

原始证据保存在私有目录`upd-maintenance-u4-20261027-fifth`，真实Root CLI成功为sixth记录，前面的失败不覆盖。当前未进行设备安装、生产签名、系统升级或ARM运行；远端CI另按对应提交核实，不能挪用旧CI。

## 下一步与成员接口

1. 主线继续把已验签的selected/candidate/fallback程序字节与实际metadata、有效systemd配置和这份持锁观察绑定，接入有限维护准入。只读报告不批准写入。
2. C仍负责UPD-02/03/05等已分配材料与健康实现；U4新增观察API不等于接管其任务。当前UPD-01整体尚未交付，C消费完整接口前仍应等主线收口通知。
3. 原P0～P7为80%，扩展14/169、剩155、估时加权7.3%保持；PMIC/关机与真人、换卡验收边界不变。
