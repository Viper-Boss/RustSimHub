# 系统集成

当前 board.toml 仅为实测硬件配置，不是刷机配置。
boot、内核、模块及恢复链固定后再生成可审核的完整安装清单。
禁止在任何 UNRESOLVED、release_ready=false 或 recovery_verified=false 时发布刷写包。

原始分区备份、基带和射频校准材料必须私有保存；本仓库不重新分发这些内容。
首个系统版本沿用已经运行的 Linux/驱动栈，Rust 替换用户态业务服务。

Debian 基础候选的真实包锁为 `base-system.lock.json`，工具见 `../tools/rootfs/README.md`。
它固定签名快照、220个包的版本/摘要、boot/modules精确哈希和Core数值身份；
基础树不是完整镜像，也没有替代本目录board.toml的发布刷写门禁。

内核基础源码归属已固定到93a71ee9468d：完整86859文件/链接的orig归档重算Git tree与该提交一致，
公开核验器为`../tools/verify-kernel-source-tree.py`，不会提取或执行源文件。
board.toml同时保留kernel_independent_rebuild_verified=false；固定上游二进制、完整源码归属、
额外22模块逐字节重建与整机冷启动是不同证据，来源字段不再UNRESOLVED不意味着允许刷写。
