# C26 精确候选健康关联提案

`crates/update/src/health.rs`是纯评估/只读探针候选，没有Root RPC、confirm调用或持久状态变更。新返回值不是生产健康回执。负责人须审核此接口后另行接纳可信生产者，不能把“候选一致”映射成确认授权。

`ExpectedCandidate::verify`调用现有精确字节签名验证及全部载荷验证，检查board/arch/epoch/schema/floor、完整文件集合和ELF/摘要。该类型字段私有且不支持Deserialize，不能从一个JSON `verified=true`构造。源码摘要与manifest摘要均取实际已验证输入。

`ExpectedFence`表示调用者当前期望boot、Root材料摘要、data代次和一次性challenge；它本身不是锁。`UntrustedObservation`名字明确表示尚无生产者身份。`correlate`检查精确候选/来源/epoch、boot/challenge、最多5秒且未过期/非未来的观察、Root/data代次/schema/兼容、Core986/Edge985/Audio984（format2还需Forwarder983）的唯一PID/role、非零start_ticks、程序hash、NoNewPrivs及空有效能力。缺失、重复或任一不匹配均拒绝。

Linux `probe_process`读取真实`/proc/PID/stat`、`status`及打开的exe文件，有限读取和hash，前后复核start_ticks、exe inode/size/mtime/ctime，拒绝消失/exec/PID变化、混合UID与过大程序。允许内核exe链接是为了读实际运行映像；不运行该程序，不信任用户提供的路径。单次探针不能锁住进程/UID变化，不能证明Core与Root跨进程事务一致，更不能替代服务端认证。

即使调用者伪造一份所有字段都匹配的观察，返回值仍为：`candidate_consistent=true`、`missing_trusted_producer=true`、`authorization_verified=false`、`may_confirm=false`。它只用于诊断、失败筛选和离线接线测试，**没有一条可确认分支**。

主线接线必须另行完成以下事项，不能用本模块填空：

1. 从真实Root/受信IPC生产者取得SO_PEERCRED/实际服务身份及活的UPD01维护守卫，原签名/nonce/floor/预算语义不变。
2. 同一候选、同一boot和数据事务的Root摘要及schema/代次来源可信，业务就绪来自真实Core检查，而非进程存活或自报healthy。
3. challenge由可信一侧生成/一次消费，过期、重放、进程更换、数据迁移失败/撤权均失效；此纯库不保存消费表。
4. 确认前重新核对候选选择/回退与全部守卫；只有主线已接受的可信生产者和政策可以调用现有Store::confirm。

实际测试使用合成签名和真实verify/verify_payload，覆盖错签/改载荷/回滚floor、旧来源/错manifest/epoch、boot/challenge、过期/未来/超长TTL、Root/data/schema、错误UID/hash/能力/NNP、重复PID/role和真实本机exe读取。不据此称设备健康、自启动、Root授权、掉电回退或自动confirm已通过。
