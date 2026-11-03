# Root 运行态只读来源组件 — 接口检查点

起始 main：`8f9cfeff1a4b5a804b8b138e890f7386caf5a006`。独立工作树及分支
`codex/web-root-runtime-observation-20261103`；开工 fetch 成功，开放 PR 195 是第一窗口音频实现，无重叠运行态 PR。本文将于实际实现/测试后补齐精确来源与结果。

本提交仅固定不可构造/反序列化/克隆的接口与拒绝样例，当前接口全部拒绝，不称组件已实现。继续实现无需等本检查点批准；整个包交统一审核。

实际复用：ReadOnlyState 持有现有共享非阻塞锁、信任/账本 inode 与 state.next 检查；verify/verify_payload 是唯一签名和载荷算法；health::probe_process 读取实际 stat/status/exe；systemd 调用复用维护组件的固定命令/环境清空与 Reap 子进程回收。运行态不会取得停机准入的数据写锁。

固定来源：账本 `/var/lib/rustsimhub/rustsimhub-v1/system/app`，信任 `/usr/share/rustsimhub/release.public`，selected `/run/rustsimhub-app/selected`，真实载荷 `/opt/rustsimhub/versions/<manifest-hash>`。选槽服务的 versions 绑定仅在该服务命名空间内，运行态不能要求外部账本 versions 目录有相同 inode。

固定角色：core.service/core/986，edge.service/edge/985，media.service/audio-alsa/984，forwarder.service/forwarder/983。采集实际 systemd MainPID、用户/状态/ControlGroup，核对 proc start_ticks/UID/NNP/能力/exe inode+摘要及 cgroup 成员，前后重读。Linux boot UUID 与 Core 64hex boot 分开；后者、业务就绪、data 代次、发起授权均无可信源，保留未验证。

有效期从 Instant 开始计算，重验不能续期；失败后对象永久失效。禁止写账本、选择、恢复、确认，禁止读业务数据库/密钥/正文。格式2文件存在不意味着转发应开启；实际 worker OFF/ON 与未知授权策略分开报告，未知关系不做健康关联。

失败矩阵计划：缺源/错信任/state.next/竞争锁；未尝试候选/越界/换链；签名/载荷/来源；PID/exec/UID/NNP/能力/cgroup/残留worker；boot/年龄/失效；systemctl超时/超量/坏输出/不可用；全程文件清单前后比较。真实 Linux 进程与合成 systemd 端口分别标注。当前尚未测试，未访问设备/他人私有材料，未安装/签名生产材料。

仓库既有 AGPL-3.0-only 代码与固定合成密钥夹具复用；AI 辅助实现及审查，保留真实 Git 身份。整体81.5%、118成员步骤、169动作及权重不变。
