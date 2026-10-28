# UPD-01 实机验证：新更新器与 admit-stage 暂存（2026-10-28）

用户批准：“把带 admit-stage 的新更新器装到设备 rootfs，并用它实测一次应用组暂存”。本次的源码都是 main `552936be3b740384abe523e341efdfaa18751140`。

## 一、替换 rootfs 上的更新器

- 现装更新器来自 `6853d71`（10-25 转发交付）。到 `552936b` 之间，`crates/update` 只有新增：准入模块、`admit-stage` 和 `inspect-maintenance` 两个命令，以及存储里两个只读的辅助函数。`boot-select`、`stage`、`choose` 和存储格式都没有改动，所以开机选择流程不受影响。
- 新程序（应用 54 构建产物中的 `rustsimhub-update`）在上传应用 54 时已经在设备的 ARM 上实际运行过：版本信息正确，当前组、出厂组和候选组都验签通过。
- 替换方式：先核对设备上旧程序的摘要与记录一致；旧程序在设备的维护目录和电脑私有目录各备份一份；新程序先写到同目录的临时文件，同步后再原子改名覆盖；替换后核对摘要、版本信息（含 `maintenance_observation_protocol: 1`），并用新程序验证当前应用组。替换后任何一步失败，都自动换回旧程序。
- **过程中的失败（都是脚本的问题，都已保留记录）**：前五次替换和检查其实都执行成功了，但我写的最后一步检查有错。“`admit-stage` 不带参数应当失败”这一步写成了 `命令; echo $?`，而远程 shell 开着 `set -e`，命令按预期失败时 shell 直接退出，`echo` 没有执行，于是被误判为失败。另外两次，一次是执行轨迹混进了 JSON 输出，一次是退出码被拆到下一行，同样都是检查写法的问题。每一次都按设计自动换回了旧程序，换回后核对摘要正确。更新器不是常驻服务，这期间没有任何服务受到影响。第六次改正检查写法后通过。
- 结果：`/usr/libexec/rustsimhub/rustsimhub-update` 现为新程序，root:root 0555。

## 二、用 admit-stage 暂存 epoch 55

候选 epoch 55 和应用 54 是同一份源码、同一批构建产物，只是用同一把设备信任密钥重新签名（版本号变了，所以清单摘要不同）。签名后用真实 ARM 更新器验签通过，3 个反例都被拒绝；上传后在设备上验证通过。之后做停机持锁业务备份，数据库完整性通过。

暂存步骤：

1. 设置一个 10 分钟的保护定时器（万一中途断开，会自动拉起应用组）。
2. 停止 `rustsimhub-app.target`，等待 core/edge/media/选择单元全部变为 inactive。
3. 用 `systemd-run` 在受限环境中运行 `rustsimhub-update admit-stage`：只授予 `CAP_DAC_READ_SEARCH`（只读打开 Core 的业务数据）、`CAP_SETUID`、`CAP_SETGID` 和 `CAP_KILL`（以 Core 用户身份运行签名程序的 build-info，超时时可以结束它）这 4 项能力；其余限制与原来的 stage 一样（ProtectSystem=strict、私有网络和设备、NoNewPrivileges，只允许写更新存储和 versions）。
4. 重新启动应用组。

**admit-stage 结果（退出码 0）**：

| 项目 | 结果 |
|---|---|
| 已安装信任（`installed_trust_verified`） | 通过，信任取自持锁的更新存储 |
| 槽位身份（`slot_identity_verified`） | 通过 |
| 有效 systemd 配置（`effective_systemd_verified`） | 通过，实际读取了 Core 单元的生效配置 |
| Root 模式 | `setup_ap`（AP 配网 Root 配置已安装） |
| 实际数据 / 预期数据 | 业务 schema 6、outbox 格式 3，两者一致 |
| 当前组 / 候选 / 候选后的回退 | 三项都是 `compatible`，没有不兼容原因 |
| 签名程序 | 三组都验证通过（`signed_programs_verified`） |
| 暂存 | `staged: true`，清单摘要与候选一致 |
| 授权（`authorization_verified`） | `false`：谁有权发起维护属于 UPD-02，按设计保持 false |

**和预期不同的地方**：第 4 步重新启动应用组时，开机选择单元发现有刚暂存的候选，就直接试运行了 epoch 55（`attempted=true`）。这是选择单元的既有设计，平时的“启动”步骤做的也是这件事。我的脚本却期望恢复成 54，所以记为失败。没有服务停摆，所有单元都是 active。之后没有再停止或启动任何服务，只把正常启动脚本里的核对逐项做了一遍：正在运行的 Core、edge、音频程序是候选 55 的签名版本，capabilities 为空，`NoNewPrivs=1`；身份不变，转发关闭，恢复预算没有动用；随后撤销了保护定时器。**以后用 admit-stage 时，暂存之后重新启动应用组就等于开始试运行，不需要再单独执行 choose。**

## 三、健康确认

floor 和 confirmed 都是 55；14 个 HTTPS 资源与源码一致；身份和所有者、95 条短信、未知回执不变；音频 7/7；转发关闭，已保存的 Bark 配置保留；普通数据关闭；出厂档位和自动恢复预算都没有动用。

## 结论

UPD-01 在 U6 记录里列出的剩余项是签名包、合并和设备交付，现在都已完成：准入命令已经在真实设备上，对真实 Root 配置、真实业务数据和 outbox 格式做了三方兼容判断，持锁通过后才暂存，暂存出的候选随后试运行并确认成功。**UPD-01 记为完成。** 不在本项范围、仍待处理的有：UPD-02（谁有权发起维护，`authorization_verified` 目前为 false）、UPD-04（在实机上注入一个失败的候选）。

全程没有拨打电话、没有发送短信或推送、没有开启蜂窝数据，也没有重启设备或写分区。私有记录：`.private/app54-updater-install-20261028-{first..sixth}.json`、`app55-*`、`app54-updater-before-20261028-*`。
