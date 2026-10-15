# 锁定 Debian 基础系统与生产程序运行

2026-10-15，在本机 x86_64 Ubuntu WSL 完成 ARM64 Debian 13 基础系统构建；没有使用棒子编译或刷写它。

## 输入与验证

复用工具原始来源为 AirSimHub `ea019485e5d890a96acae357c52b94c746de837f`，逐文件摘要、修改与许可见 `tools/rootfs/README.md`、`reuse-provenance.json` 和顶层 NOTICE。不是复用旧 Go Agent 作为 Rust 实现。

`firmware/base-system.lock.json` 从 `20261001T000000Z` Debian/debian-security 快照实际 bootstrap，gpgv 验签 InRelease，并核对 Packages 的长度和 SHA256。解析完成220个包的精确版本、来源和deb摘要，包集合摘要为 `cdd70c5a3c412e9ff6d5a7b9bda8ffc03e68c1620f57bb17a084f74d93a00ada`。锁文件 SHA256 为 `bb040be5491f6098b73c098110671c7bf2f89be46f9942bc342b5070b67e8190`。

包含维护 SSH、时间同步，省略非首版范围 Bluetooth；dnsmasq-base 只提供后续热点依赖。构建不下载内核/厂商固件，不含任何个人固件、NV、设备身份、维护公钥或网络密码。

输入boot为当前棒子完整64MiB备份的 `d5790e07...`，模块归档为此前验证材料 `7e1d5d53...`；展开的全部文件清单摘要与当前棒子 `1552358f...` 一致，703个模块逐个vermagic匹配。精确值见已提交的包锁与board.toml。没有因此把内核源码/完整恢复门禁标为已完成。

## 实际结果

成功构建的工具源码提交为 `452d75e9f647b01ca2badb80b8e980ee99cec96b`，Git归档到新的Linux私有工作区后执行。实际安装包集合与锁完全一致；Core账户986:986、nologin及锁定密码通过审计，构建日志、SSH host keys、随机machine-id/seed等清理完成，通用根文件系统审计通过。

该提交的GitHub CI `38732828672` success，包含33项rootfs核心测试及现有Rust workspace/production检查和lint；这不替代镜像/真实启动验收。

| 项目 | 结果 |
|---|---|
| 根文件系统条目数 | 12,264 |
| 树 SHA256 | c6992751d2d246fd423c3b0680fa47d622c711ec2cab494b5ded7c8da279b7c6 |
| 归档字节数 | 349,624,320 |
| rootfs.tar SHA256 | f1334e41580cd4a27404aabb896b3f4805ea3ffaffafd5b75958e93b95a07216 |

首轮冻结构建使用6291d9a，已安装/清理正确Core账户，但审计器仍查旧的单服务账号，构建拒绝并保留 `output.FAILED`，没有发布为可用输入。修正账号审计后从452d75e重新完整构建成功，新增旧账号、UID/GID为0、可登录shell拒绝测试，当前33项核心工具测试通过。前两次私有工具预检还分别发现缺少公开测试指纹/imagepolicy依赖，在bootstrap前停止；失败日志保留。

两次独立bootstrap清理后的完整树实际比较一致、差异条目0。第一轮原审计结果仍是失败；这只补充树复现证据，没有冒称两份最终归档已逐字节比较或跨日期/构建机复现通过。

`tools/test-base-runtime.py` 另核验已实机验收的cd4c96e六个生产ARM64制品摘要/来源。在独立mount/network命名空间中，只读绑定新基础树和制品，以986:986通过六程序build-info、Core无硬件self-check及CLI help；限时退出。没有挂入硬件或D-Bus、没有网络访问。私有报告passed=true、base_tree_unchanged=true，Core development_fixtures=false，六程序来源均为 `69a2450c5419844975ed1960efd2c27900c444e870a393b93672e4c7aeaa2fa7`。

## 证据与边界

私有文件：`base-system-p6d-resolve-c.log/.exit`、`base-system-p6d-build-first.log/.exit`、`base-system-p6d-build-second.log/.exit`、`base-system-p6d-build-second-manifest.json`、`base-system-p6d-tree-comparison.json`、`base-system-p6d-runtime-first.json`。成功基础树在本机WSL `/home/wzs11/.cache/rustsimhub/base-system-p6d-build-second/output/`，不要清理或当作已安装系统。

这是带匹配模块的干净基础输入。尚未加入Edge/Media/IPC账户及Rust服务组、板级固件加载/USB/持久分区、首启与证书交付，不是完整固件，不可直接刷机。没有验证物理上电、NAS关闭、首启、通话/短信写入、热点、蜂窝或完整系统回刷。本轮硬件新增通话/短信/蜂窝测试为0；恢复入口演练另见P0C。

P6增加0.5点至3.5/12，总体68/100。其他已计功能不重复增加，100%目标和流量换卡约束保持。
