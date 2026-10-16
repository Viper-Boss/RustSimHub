# 首板9008只读捕获与返回验证

2026-10-16，用户在NAS旁完成按键插入，NAS在预先记录的同一物理USB口实际识别到05c6:9008。此轮没有刷写、擦除或修改棒子的分区，也没有拨号、短信或普通蜂窝数据测试。

进入前核验旧四服务active、PCM关闭、电话空闲、普通蜂窝断开、当前boot摘要和设备身份。只发起一次正常poweroff，观察普通USB消失后才给用户同口按键插入步骤。物理动作不能证明文件系统已干净卸载，后面的实际检查发现原图需要journal恢复。

Root私有NAS协调器先检查固定工具/加载器/源码摘要，再用受限封装验证主备GPT及其CRC、准确分区布局和既有GPT摘要；随后只读捕获完整3,909,091,328字节eMMC主区，确认两份GPT和当前boot摘要。原图SHA256为 `264c5c5cccdc894b5fa381aaa2b79a878120fe0cc8bd672bb5ec7a936be2f3cf`，NAS独立整图回读匹配。boot0/boot1仍是此前独立核验的分开备份，本次没有通过EDL重新捕获它们。

协调器只做一次正常reset并获确认，退出0。随后通过固定SSH主机密钥重新连接，确认boot ID已改变、boot和旧身份摘要保持、旧四服务active、电话空闲、PCM全关闭、USB/Wi-Fi恢复且普通蜂窝仍断开。reset确认与实际返回是两项分开的证据。

原图Root600、保持不变。NAS e2fsck1.47.0对rootfs和NSHSTATE的只读检查均退出4，包含未回放journal和orphan/元数据计数问题，**原始捕获未通过文件系统一致性检查**。第一份独立副本仅回放journal后仍失败；第二份自动preen跳过了标记clean的状态分区完整检查。上述副本、报告和失败日志全部保留。

第三份独立副本使用 `e2fsck -f -p`：完整检查并仅自动修复安全问题，不使用强制答yes。两个分区各退出1，随后各自只读 `e2fsck -f -n` 均退出0，所有临时loop已分离；整图GPT及boot未变，原始捕获摘要再次保持。该**经journal与元数据修复的派生副本**为3,909,091,328字节、SHA256 `962ad1ab1a569afca308825d501c8068e957d729595057d3f65492f254662da4`。不能把派生副本成功改写为原图一致，或当成整机回刷已验证。

私有索引：`.private/recovery-edl-20261016-first.json`，只读失败详见 `recovery-edl-offline-check-details.log`，journal-only和普通preen失败各有独立日志，最终 `recovery-edl-fullcheck.log` 保存派生核验报告。NAS位置由私有索引定位；原图、各副本、工具、加载器、设备身份和完整日志均不进Git。

P0从6增至6.5/8，仅计入实际9008进入、受限只读捕获和返回。完整镜像回刷、实际恢复闭环、精确内核/DTB源码归属仍未完成；`recovery_verified`、`flash_allowed`、`release_ready`、`board_boot_verified`保持false，内核来源仍UNRESOLVED。
