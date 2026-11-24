# 2026-11-24 双槽刷机发布预检

## 已验证

从已存在的 UFI003 单槽刷机包读取签名清单，使用 UsbTool 预置的公钥验签，并逐块核对临时 rootfs 副本和清单的 SHA-256。旧镜像为 1,456,963,584 字节。该副本用 `resize2fs` 缩至 1,392,508,928 字节（1328 MiB）。缩小前后的 `e2fsck -fn` 均退出 0，12,407 个文件和 110,821 个已用块一致。该检查确认旧 ext4 内容可以装入 4 GB 双槽布局的任一槽。临时副本保留旧文件系统标识，仅作为容量证据。

读取同一个签名包的 boot 头，启动参数为 `root=LABEL=rootfs`。刷机包清单只列出 `aboot`、`boot`、`NSHSTATE`、`rootfs`。它使用原单槽布局，不能用来安装双槽系统。检查期间未访问或改写任何棒子。

## 发布边界

RustFlashKit 使用 GitHub Actions 构建 Windows x64 工具预览。流程固定工具源码和上游 EDL 归档摘要，执行测试与打包后自检，发布图形版、CLI、校验文件和对应的 EDL 源码。高通 Firehose 加载器由使用者另外选择。预览提供型号、容量与只读 A/B 布局，目标设备的实时 USB 维护需要相应的候选固件。

A/B 整机包暂未形成。真实的早期选槽、试启动失败回退、共同数据分区挂载、首次初始化和刷机工具的 A/B 写入还未接通。待验证的设备路径是单台 UFI003 的首次启动、完整读回、断电重启和失败回退，8 GB 另外需要真实分区表核对。

## Windows 工具发布结果

GitHub Windows x64 构建通过 257 项测试。两个 EXE 已打包，命令行包的只读护栏及 Windows libusb 自检通过。公开发布页回读确认两个 EXE、SHA-256 校验文件和上游 EDL 源码，清单与 GitHub 资产摘要一致。[Windows 工具预览发布](https://github.com/Viper-Boss/RustFlashKit/releases/tag/v0.1.0-ab-preview.1) · [构建日志](https://github.com/Viper-Boss/RustFlashKit/actions/runs/45331131839)。
