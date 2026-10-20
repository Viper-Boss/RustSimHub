# 完整基础内核独立构建 P6T

2026-10-21。本机WSL在新私有输出目录完成 `Image.gz modules dtbs`。没有在棒子编译、打开设备、生成启动镜像或写boot；本节独立于第二次rootfs/状态分区安装。

## 输入与实际结果

- 固定提交 `93a71ee9468dd8d47a9e082e5d818608f08712e3`，完整解包源码树在构建前后均为 `72d4e8e42649f763dd969a1de893051ed64387a1`。原包按 `firmware/kernel/upstream-source.lock.json` 核验。
- 上游image包内配置SHA `a326c3b693925fa0edb0dde9fb90a4d0655a740d096f719a5aa3fc020fd0a04b`。成功构建只改变 `CONFIG_CC_VERSION_TEXT` 中入口名的 `-14` 后缀，功能选项保持，包括RELR。
- 私有解包的Debian GCC `14.2.0-19cross1`（横幅 `14.2.0-19`）及binutils2.44，下载经过Debian trixie APT签名和包摘要验证。动态/静态ARM64链接及内核自带RELR探针通过；没有安装宿主系统包或全局替换libc。
- 实际完整构建退出0，12个并行任务，含配置、来源复核和产物检查约193.5秒；生成681个模块及所选配置DTB。release为 `6.12.49-msm8916-g93a71ee9468d`。
- 原headers与新构建的15468项导出符号表一致。原配置没有非零模块CRC，**不能称为CRC兼容性已验证**；工具链版本修订相同也不证明编译器可执行文件逐字节相同。

| 产物 | 字节数 | SHA-256 |
|---|---:|---|
| Image | 17397768 | `686f502842614faf978d398b313cffde47292bcdfdd5c9a6c0e095dae0144d0e` |
| Image.gz | 7489045 | `fe6c912931c7ad75c501e627102d701b3e9e8c8b04f7450bb2b0fc1539f1d8d3` |
| vmlinux | 22295160 | `63b0168c24ac58fb7fec1f6a874c099968f46e18826d612737e529093b71b8c7` |
| Module.symvers | 982078 | `b51611eb8cc4ce2c0ad7e3105fa223580458e62f6070cc53909e92eb95af79b2` |

成功构建的压缩/未压缩内核均与上游字节不同。另一份全新sixth输出使用上游用户/主机/时间元数据和编译器入口名，完整配置与版本横幅均精确相同，仍有14943个未压缩字节不同；其Image SHA为 `6c893bef787d219e63224a86ca45ac985be7628469d6d883fe826972ced367ec`，Image.gz SHA为 `e862aa7419dbf7cd7cb79e55336ae228441c1cbba0ef5567b5da55fc62cd8575`。该复现尝试实际退出0，约192.8秒，同样681模块、导出表一致及来源树保持，但**逐字节复现仍未通过，不能将差异仅归为横幅**。第五份使用本项目构建身份的原产物保持；sixth的上游元数据只用于对照，不冒充原上游构建或发布制品。

## 复现要点与失败记录

1. 先核orig/image包摘要，再按Git blob、mode和原始链接文本核完整树。Python3.14安全tar filter规范化一个带 `./` 的链接文本，第一轮被树门禁拒绝；后续仅在filter验证且路径分量证明只是等价规范化时保留原文本，没有关闭安全过滤。
2. 原配置放全新 `O=`，先 `olddefconfig` 并拒绝功能变化。原Ubuntu工具链缺ARM64开发库时链接选项消失；补齐后binutils2.42仍使RELR消失，两轮均在编译前拒绝，没有删功能放行。
3. 使用签名校验的Debian工具链、目标libc开发文件及私有native bison/flex/libelf/OpenSSL头/库。全局不加入整套旧宿主libc。一次不存在宿主make的准备失败保留，成功构建使用既有私有make。
4. 设置 `ARCH=arm64`、原 `CONFIG_LOCALVERSION=-msm8916`、`LOCALVERSION=-g93a71ee9468d`、固定工具链 `CROSS_COMPILE` 和带私有sysroot的 `CC`，运行 `make -j12 Image.gz modules dtbs`；随后重核来源树、产物、release与导出表。

私有证据：`full-kernel-rebuild-20261021-fifth.json/log`、`full-kernel-rebuild-20261021-sixth.json/log`、`debian-kernel-sdk-20261021-first.json/log`、`full-kernel-deps-20261021-first.json`、`full-kernel-binary-comparison-20261021-first.json`和`second.json`。第一至第四轮失败和提取差异报告均保留；这些新构建不替换正在运行的冻结安装输入。

完整initramfs源码重建、新内核与固定音频驱动/DTB组合、真实启动及恢复仍待验收；不打开 `flash_allowed`、`release_ready`、`board_boot_verified` 或 `recovery_verified`。整体仍74/100，不凭离线构建重复加点。
