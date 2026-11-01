# 主线 ARM 代码构建检查点（未签名、未部署）

固定主线：`dc3aa493d84190248d26587ca662a197414d2958`。源码 tar SHA-256：`5207f560fc4affd92f328d67d766dd9aa22f8f94c2b77d8c8ef426202b6e1bc2`。

## 已完成

- #151 修正隔离安装模板及逐测试副本清理；本机真实 Root/UID/chroot 10 项通过。#150 的 C22/C23 测试合入后，当前热点 crate 的 180 项全部通过。
- 对固定提交的 1202 个文件逐字节核对 Git blob，排除原工作目录的未提交修改；复用既有 Rust 1.98.0 和交叉编译器，在本机普通用户 WSL 的新空 target 中离线构建生产功能集合。
- 12 个 `aarch64-unknown-linux-gnu` 主程序通过 ELF 架构、SHA-256、GLIBC 不高于2.41的检查：Core、Core launcher、Edge、转发器、mDNS、迁移、网络代理、恢复、热点助手、灯控、更新器、simhubctl。
- Core/Edge/update/setup-ap/Core launcher 的 QEMU `--build-info` 对应同一源码摘要；开发测试开关关闭，数据库 schema 均为6，配网契约一致。
- 同一份源码另用新空 target 构建原生音频程序，既有音频 sysroot 的文件摘要重新核对。QEMU 实际执行23+11=34项测试，0失败、0忽略；ARM音频产物的架构、GLIBC、源码标识和平台路由能力核对通过。
- 共13个 ARM 程序产物，不只是宿主机通过；没有在 NAS 或棒子上编译或执行。

私有构建报告与产物清单保存在主工作目录 `.private/main-arm-build-20261101-dc3aa493.json`、`.private/main-native-20261101-dc3aa493.json`，相应 artifacts 目录保存实际程序。脚本、原始日志和未签名产物不公开。进度看板只发布脱敏结论和固定提交。

## 仍需继续

最新主线完整 GitHub CI run `41286902279` 已回读为 completed/success，HEAD 精确对应上述 `dc3aa493`；`test` 和 `native-audio` 两项均成功，含此前失败的chroot、后续升级兼容/真实转发协调、生产功能集和全workspace严格Clippy。修正分支的两轮完整CI `41286853193`、`41286848801` 也成功。前一次 run `41275851553` 仍明确保留为失败，不能覆盖历史结果；修复后以新运行的证据证明通过。

还未组装/签名应用候选、未生成新 rootfs 镜像、未做设备安装、冷启动或手机配网验收；这些 ARM 程序不能直接当作完整可刷固件。人工测试按用户要求放最后，PMIC/关机继续搁置，成员草稿继续保留。

原P0–P7验收保持 **81.5%**，本轮构建与PR合并不增加实机点数。五人代码量另外按上述主线SHA重算，不将行数占比作为验收比例。
