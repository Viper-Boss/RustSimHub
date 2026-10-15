# Debian ARM64 基础系统构建

这是完整 Rust 固件的基础输入工具。它不写设备、不安装 Rust 服务组、不生成可刷完整系统；`ROOTFS-MANIFEST.json` 中的 `release_input` 只描述 Debian 基础输入的锁定状态，不能代替顶层 `firmware/board.toml` 的恢复/发布门禁。

代码来自 AirSimHub `ea019485e5d890a96acae357c52b94c746de837f`，每个原文件的 SHA256 见 `reuse-provenance.json`。调整了品牌、服务目录/用户名和包请求；去掉上游测试中仅针对旧项目发布文档及 SBOM 装配器的两个类，保留全部 32 项核心测试。脚本/配置为 AGPL-3.0-only；`kernel/audio/replace-appended-dtb.py` 保留 GPL-2.0-only，许可全文在顶层 `licenses/GPL-2.0-only.txt`。没有复制或分发内核制品、厂商固件、NV、维护密钥或用户资料。

运行环境：本机 Linux/WSL，root，mmdebstrap、ARM64 qemu-user/binfmt、Debian archive keyring、gpgv、Python、GNU tar。在 Linux 文件系统中构建，以保留 owner/mode/symlink；不要让棒子承担编译。下载仅从锁定 Debian/debian-security 快照完成，保持签名校验。过期快照的 Valid-Until 检查单独关闭，不能关闭包/索引签名。

已解析锁为顶层 `firmware/base-system.lock.json`；本目录 `firmware/rootfs.lock.json` 是用于解析及拒绝未解析输入测试的模板，不能拿模板生成发布输入。新的请求包括维护 SSH、时间同步，省略非首版范围的 Bluetooth；`dnsmasq-base` 只是热点候选依赖，热点实际控制/硬件验收仍待完成。

```sh
cd tools/rootfs/scripts
python3 -m unittest test_rootfs
cd ../../..
sudo sh tools/rootfs/scripts/build-rootfs.sh \
  --lock-file firmware/base-system.lock.json --setup-ap \
  --boot-img /private/verified-boot.img \
  --modules /private/verified-modules.tar.gz \
  --out /private/new-base-build
```

boot/modules 输入必须是锁中精确哈希；模块逐个核验 vermagic。每次输出必须是新目录。构建之后核对实际包版本集合，清理构建日志、随机 machine-id/SSH host keys、网络凭据等，执行通用根文件系统审计并生成树/归档摘要。

要刷新锁，复制模板到新的评审文件，在已审核的包请求基础上运行 `build-rootfs.sh --lock --timestamp ... --setup-ap --lock-file ... --boot-img ... --modules ... --service-uid ... --service-gid ... --image-size-mib ...`。解析会实际 bootstrap，再验签 InRelease 和核对 Packages 摘要；审核结果、提交后再做冻结构建。固定 UID/分区大小须来自目标设备实测。当前只创建 Core 的固定账户；Edge/Media/IPC、持久分区及首启/生产服务组由下一层集成，不能把这个基础树直接刷到设备。

是否逐字节可复现要以实际重复构建比较为准；声明锁定输入、测试通过或字段 `reproducible=true` 均不单独代表硬件启动、完整镜像或重复构建已验收。完整安装包仍须完成许可清单、固件供给、首启、完整恢复和最终实机验收。
