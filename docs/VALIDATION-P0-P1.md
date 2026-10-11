# 首轮验证记录

日期：2026-10-11–12。当前交付为开发基础与只读原型，不是完整固件发行版。

## 构建与源码

- 首次代码提交：`154d654`，crates / Cargo.toml / Cargo.lock 内容与 NAS 源包一致。
- Rust 最低工具链声明及 CI 固定：1.98.0。
- NAS 实测构建工具链：rustc 1.99.0 / cargo 1.99.0（镜像中的稳定工具链）。
- 构建架构：Linux aarch64，官方 Rust bookworm 镜像。
- 镜像 digest：`rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa`。
- 源包 SHA-256：`91c7e8e008c0749dcca5f2bfdfcd14739334f262f143d14a33e7427db784d40f`。
- ARM64 程序：1,972,904 字节（约 1.88 MiB），ELF aarch64 PIE，动态链接 glibc，已剥离符号。
- 程序 SHA-256：`d9f5fa44d7ed79badad20dce478421f002805e40e5ffc5e865b1fa210ef72984`。
- NAS 容器退出码 0；测试构建约 5m16s，release 构建约 8m16s。这是受限于 2 CPU 的首次构建，不能当作增量编译性能。

## 自动检查

| 环境 | 命令/检查 | 结果 |
|---|---|---|
| NAS ARM64 | cargo test --workspace --locked -j 2 | 12 项通过 |
| NAS ARM64 | cargo build --release --workspace --locked -j 2 | 通过 |
| NAS ARM64 | rustsimhub-core --self-check | 通过 |
| GitHub Linux x86_64 / Rust 1.98 | fmt、workspace tests、Clippy -D warnings | 通过 |
| 本机 Windows | cargo fmt --all -- --check | 通过 |
| 本机 Windows | 原生编译 | 缺少 link.exe；未通过 |
| 提交内容 | 隐私审计与 git diff --check | 通过 |

首次 CI：[运行 38153648494](https://github.com/Viper-Boss/RustSimHub/actions/runs/38153648494)。
Windows 缺少链接器属于开发机配置问题，ARM64 和 GitHub Linux 验证已独立完成。

## 棒子实测

执行 tools/device-smoke.sh，SSH 退出码 0：

- `--self-check` 通过。
- root 与 nassimhub 非 root 用户均成功查询 system D-Bus / ModemManager。
- 查询到 1 个 modem，state=8，access_technologies=16384，Voice/Messaging 接口存在。
- 带 PrivateNetwork、PrivateTmp、ProtectSystem、NoNewPrivileges 等限制的临时服务成功启动。
- 合法本机 UID 的 health/capabilities 请求成功，响应只含白名单字段。
- root 虽可通过 DAC 访问 socket，但不符合配置的 peer UID，被核心拒绝，客户端收到连接重置。
- RSS / HWM 样本均 3400 KiB（约 3.3 MiB）；这是两个只读请求之后的单次样本，未做完整业务性能测试。
- nassimhub-agent、ModemManager、NetworkManager、rmtfs 保持 active。
- 临时单元已停止，未安装开机服务。

首轮临时服务因 PrivateTmp 隐藏 /var/tmp 中的执行文件而无法启动。
调试脚本改为安装到独立的 /opt/rustsimhub/dev-p0，并保留 PrivateTmp；重测通过。
脚本同时增加 EXIT 清理，异常路径也停止临时测试服务。

## 私有备份

64 MiB boot 分区复制到本机与 NAS 私有目录，两端 SHA-256 均等于 firmware/board.toml。
NAS 副本权限 0600，目录权限 0700。原始内容未进入 Git。
完整 eMMC/rootfs/业务数据备份及恢复验收仍未完成。

尚未验证新 Rust 程序的短信收发、拨号/接听、声音、蜂窝数据、远程访问和掉电恢复。
