# 板级候选树与实际挂载守卫

2026-10-15。此阶段完成内部系统树组合及板级准备验证，没有刷写或安装生产应用自启服务。

## 冻结输入与输出

最终组合源码 `724fc0922794ad08214b574b9c63bb438d808f1f`，采用P6D的锁定220包基础树（构建源码452d75e）及P6E的ca596a7六程序（源归档 `8bdf8fcb6b9ebce139a8c0c4fd8de742fd3a17b8690d729f3506c0bbdd999f52`）。不同层各自保留来源，不把新的组合/文档提交当作新编译应用。

组合器从精确Git blobs安装overlay，保留numeric owners，核对基础完整树/已提交锁与六程序摘要及ELF架构。增加固定Edge/Media/IPC身份及组成员、factory只读程序、板级服务顺序、空持久挂载点和公开USB模板。通用根文件系统残留审计通过，原基础树摘要保持。

最终树条目12303，SHA256 `fbcbade0f7137cec59c0eeead9ec092e2e526e70e62550e0b569c5cdc228dd9b`。本机WSL位置 `/home/wzs11/.cache/rustsimhub/system-tree-p6f-network/`。外部SYSTEM-CANDIDATE.json与树内标记均明确flash_allowed/release_ready/recovery_verified为false，app_service_group_installed/native_media_installed/board_boot_verified也为false。不是可刷完整固件。

首次组合在创建输出前发现工作树CRLF和Git归档LF造成锁摘要不一致，拒绝继续；修复为读取已提交对象后组合成功。第一份6899b6a候选保留在system-tree-p6f-first，不含后续NM持久化修正，不能替代最终network版本。

## 验证

- 本机23项隔离板级测试通过：固件白名单/摘要/损坏/链接/不同既有文件保护，remoteproc已运行不重启/仅已知offline启动/crashed拒绝，挂载失败不准备目录、旧文件内容/owner保持、私有角色目录、稳定USB serial/pending续用、损坏/链接拒绝，以及NM profile只种入一次、保留既有NM补充内容、损坏pending/链接拒绝。
- 最终树的systemd-analyze verify退出0；无依赖循环或执行路径错误。新版WSL工具对Debian原装networkd/timesyncd的旧`!!`修饰符给出两条兼容提示，未篡改原装单元，也没有冒称已启动服务。
- 在独立mount/network namespace内只读挂载最终树，以986:986执行factory实际安装路径的六程序build-info、Core无硬件self-check和CLI help。Root0555/逐程序摘要/来源核验通过，树前后相同。报告system-tree-p6f-network-runtime.json passed/base_tree_unchanged=true。
- 在实际UFI003上，只运行新state-check守卫：私有mount/network namespace中临时shadow `/var/lib`，空目录拒绝；同一真实NSHSTATE整分区绑定到新位置后接受；该绑定单独改只读后拒绝。没有运行firstboot、USB或radio。退出后新挂载不留在主机，旧identity摘要和旧挂载根owner/mode保持，旧四服务active、PCMclosed。私有state-guard-p6f-first.json passed/cleanup_verified=true。

实际设备守卫接受了板型、mmcblk0p15、264159个512字节扇区、ext4/NSHSTATE、rw及不同于根文件系统的检查；没有验证板级脚本在新系统真实开机时的行为。

## 仍待完成

下一步接入完整Core/Edge应用启动、可信本地设置与TLS/维护入口，完善媒体、签名更新组和完整恢复。NM profile已改为持久分区内Root私有目录，系统服务绑定该目录，但没有把离线测试当成新的系统实机掉电/升级配网保持证据。没有向镜像注入厂商固件、身份、设置码、维护密钥或网络密码，没有新短信/拨号/蜂窝测试。

固定总分仍68.5/100、约69%；本阶段先补齐候选层，不重复给已计的基础树/程序运行增加点数。恢复与发布刷写门禁保持。
