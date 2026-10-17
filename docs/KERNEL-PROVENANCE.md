# 当前内核与音频DTB来源对照

2026-10-17。仅做下载、离线读取和重建，不访问或写入棒子。board.toml的kernel_source_commit仍为UNRESOLVED：本节证明已记录boot备份中部分组件的对应关系，不替代完整boot/模块来源与恢复验收。

固定上游为[KyonLi内核提交93a71ee9468d](https://github.com/KyonLi/ufi003-kernel/commit/93a71ee9468dd8d47a9e082e5d818608f08712e3)，GitHub API标记提交签名verified；其Git tree为72d4e8e42649f763dd969a1de893051ed64387a1。[6.12.49-1发布](https://github.com/KyonLi/ufi003-kernel/releases/tag/6.12.49-1)的boot、完整orig源码、Debian源码补充、dsc/buildinfo、headers/image包共七项均已按上游SHA256核验。公开输入锁见firmware/kernel/upstream-source.lock.json，未提交二进制或设备备份。

| 对照项 | 结果 |
|---|---|
| 已记录boot备份 | 整体摘要与既有board.toml记录一致 |
| gzip内核 | 与上游boot逐字节相同；SHA256 1fbbef6ccaff103af9dc3281aacaa3d81e82e001d5957c4fa6a018faed1f767f |
| 解压ARM64 Image | 相同；SHA256 3474aab48d9ff0c9aecfd70201d932a0520f0c93ad416044711ab2644ec8b004 |
| initramfs/second stage | 逐字节相同；未独立从源码重建initramfs |
| DTB | 备份区别于上游，但由固定音频源码重建后逐字节一致，52161字节/SHA256 afb1027cc0371d491349dba868bdbabb927ac87c71f8a447dc504303f93f7425 |

DTB源码来自AirSimHub固定提交ea019485e5d890a96acae357c52b94c746de837f，五项原始Git blob与SHA见firmware/kernel/audio/source.json；保持原许可证/SPDX，不重新许可为Rust业务代码。DTC 1.7.2与CPP 15.2.0离线完成重建；DTC及libfdt仅下载解包到WSL私有工具目录，版本1.7.2-2ubuntu1，未安装到系统。未复制厂商固件、NV、校准或私人设备状态。

仓库工具只接受摘要匹配的上游DTB和固定源码，输出必须新建；源码/绑定变化、输入不匹配、既有输出、重建结果不匹配都拒绝，失败目录保留。不会读取设备、重打包boot或写分区：

```sh
python3 tools/rebuild-audio-dtb.py \
  --input-dtb /private/upstream.dtb --out /private/new-dtb-rebuild
```

upstream.dtb从已核验上游boot中只读提取，相关旧解析器为AirSimHub同一提交的kernel/audio/replace-appended-dtb.py；输入DTB固定SHA为5c5a12a8237af1d5451774f497dbd75f0c610134db8ec5d9590d741871afab19。仓库新增工具在当前源码上已实际重建并cmp到备份DTB通过；44项板级回归在umask077下通过，包含源码/绑定篡改、profile越界、错误上游和既有输出拒绝。原测试夹具曾因mkdir受umask影响而失败，显式设置合成runtime0755后回归通过，生产恢复脚本未放宽检查。

私有证据在kernel-source-20261017-first：acquisition.json、boot-comparison.json、dtb-comparison.json，以及最终有界读取版本的repository-dtb-final/DTB-REBUILD.json、repository-dtb-tests-final.log/exit0。初次缺DTC和夹具权限失败日志保留；一次Windows补跑被沙箱临时目录权限拒绝，固件验收以Linux的44项及实际重建为准。不能把GitHub verified当成独立核验其签名密钥或完整本地内核重现。

后续仍须核对完整上游源码与额外音频模块的实际对应、保留补丁/构建工具来源，以及boot头/root选择的重打包配方。未独立重建完整内核/initramfs，未验证完整新系统启动、物理掉电与回刷；所有flash_allowed/release_ready/recovery_verified/board_boot_verified保持false，分母与进度73.5/100约74%不变。
