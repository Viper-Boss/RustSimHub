# RustSimHub UFI003 第一阶段引导

面向 MSM8916 的 UFI003。以开源 LK 的 `lk1st-msm8916` 作为底层，固定源码到 [lk2nd 21.0](https://github.com/msm8916-mainline/lk2nd/tree/c7a0aa2fc316f8284b7bcefe14aaef0dd11f04cb)。板级设备树针对 `thwc,ufi003`，GPIO 参考公开的 [UFI003 板级移植](https://github.com/KyonLi/ufi003-debian/tree/b8c4691b2e6b885ca4d45f220a241ceabf9a991a/aboot)。

Fastboot 可用 `getvar rustsimhub:bootloader` 和 `getvar rustsimhub:storage-layout` 读取构建版本和分区布局。Linux 可从设备树 `/chosen` 的 `rustsimhub,bootloader` 和 `rustsimhub,storage-layout` 读取同样的信息。布局值有 `legacy`、`incomplete-ab` 和 `ab-v1`。A/B 选槽和试启动回退继续由 RustSimHub 的 initramfs 完成。

## 构建

需要 Git、GNU Make、ARM bare-metal GCC、Device Tree Compiler 和 GNU tar。

```bash
git clone --depth 1 --branch 21.0 https://github.com/msm8916-mainline/lk2nd.git
firmware/ufi003-stage1/build.sh ./lk2nd ./out
```

脚本从锁定的源码构建并校验 ARM ELF 与版本标识。产物是开发用 ELF，目标设备的引导镜像格式、签名信任和真机加载需要单独验证。补丁应用到 LK 源码后，编译产物遵循 LK 上游的许可。
