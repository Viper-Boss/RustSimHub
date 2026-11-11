# 通用刷机包

把任意原厂 UFI003 刷成 RustSimHub 用的 `.rshpack`，配合 RustSimHub-UsbTool 的“一键刷机”。**还没有在真机上启动过**，验证范围见 [验证记录](VALIDATION-GENERIC-FLASH-PACK-20261107.md)。

## 包里有什么

| 镜像 | 字节 | 内容 | 单台设备的数据 |
|---|---:|---|---|
| aboot | 1,048,576 | lk1st，尾部补零 | 无 |
| boot | 67,108,864 | 通用 boot（SHA `d5790e07…`，与 `firmware/board.toml` 一致） | 无 |
| NSHSTATE | 135,233,536 | 空 ext4，只有维护公钥和首次开机请求标记 | 无 |
| rootfs | 1,456,963,584 | 已审计系统树 | 无 |

NSHSTATE 分区是 264159 扇区（135,249,408 字节）。文件系统取 16 KiB 整数倍后比分区短 15,872 字节，末尾 31 个扇区不用。`rustsimhub-state-check` 只查分区的序号、名称和扇区数，以及挂载的文件系统，文件系统比分区短不影响；隔离环境里已用真实脚本确认。

## 首次开机做什么

镜像不带身份，以下内容都由设备开机时生成或读取：

| 内容 | 来源 | 负责的脚本 |
|---|---|---|
| 基带和 WCNSS 固件（`modem.*`、`wcnss.*`、`mba.mbn`） | 自己的 `modem` 分区，经只读 loop 挂载，逐段核对镜像自带的 SHA-256 哈希表 | `rustsimhub-firmware-extract` |
| Wi-Fi NV（`WCNSS_qcom_wlan_nv.bin`） | 自己的 `persist` 分区，用 debugfs 读取，不经内核挂载，因此不会回放日志 | 同上 |
| USB 序列号和 USB MAC | 随机生成 | `rustsimhub-firstboot`、`rustsimhub-usb` |
| SSH 主机密钥 | 设备上生成 | `rustsimhub-ssh-prepare` |
| 设备身份、TLS 证书、一次性设置码 | 设备上生成 | `rustsimhub-generic-install` 调用现有的 `rustsimhub-install-initialize` |
| 设备名 `rustsimhub-XXXX` | USB 序列号前 4 位 | `rustsimhub-generic-install` |
| Wi-Fi、蓝牙 MAC | lk1st 启动时写入设备树（二进制里有相应字符串，实际取值未在真机确认） | lk1st |

这两个新服务只有在 NSHSTATE 里有请求标记时才运行，已有设备不受影响。

- **不写 NV 和校准分区：** 提取只打开 `modem` 和 `persist`，都是只读。不复制原设备的 `local-volte`，也不放 `carrier-voice.enabled`，rmtfs 走默认的 `rmtfs -r -P -s`。
- **固件校验失败：** 不发布任何内容，下次开机再试。USB 和维护 SSH 不受影响，基带和 Wi-Fi 保持未就绪。
- **`WCNSS_qcom_cfg.ini`、`WCNSS_cfg.dat` 不带：** 它们原本在 system 分区，而新分区表删除了 system。系统内核模块的固件名只引用 `WCNSS_qcom_wlan_nv.bin`，不引用这两个文件。

## 维护 SSH 公钥

`tools/system/build-generic-state.py --maintenance-public-key <公钥> --expected-public-sha256 <摘要>` 把公钥写进 NSHSTATE 的 `ssh-access/authorized_keys`（root 0600）。

- 只接受一行不带选项的 `ssh-ed25519`，摘要必须与传入的值一致，私钥文件会被拒绝。私钥始终只留在电脑上。
- **同一个包刷出来的所有棒子都信任这把钥匙。** 所以这个包只能自用；要给别人用，必须换成对方的公钥重新生成。
- 主机密钥在设备上生成，无法预先写入 `known_hosts`。第一次连接只在 USB 直连线上进行，先记下指纹，以后严格核对。
- 换钥匙：SSH 登录后替换 `authorized_keys`。

## devinfo 不需要清零

结论：**不需要，UsbTool 不用加清零步骤。**

- lk1st 基于高通 LK，按 lk2nd/CAF 的通用逻辑，只有开头 13 字节是 `ANDROID-BOOT!` 才采用 devinfo 内容，否则按默认值处理。这条逻辑来自公开源码的通用结构，**没有反汇编这个二进制逐条确认**。所以全零和其他残留数据走的是同一条路径。
- 实测：新棒子该位置 2026-11-01 读出来就是全零（UsbTool 写入报告里的摘要等于 1 MiB 全零的摘要）。lk1st 在这种状态下正常进入了 fastboot，并且之后 devinfo 仍为全零。
- 仅剩的理论风险是残留数据恰好以这个魔数开头。UsbTool 可以选择加一个只读预检，这不是必需项。

## 生成步骤

全部在隔离环境进行，不接触设备。组合系统树和生成镜像需要 Linux root（本次用 `wsl -u root`）。

1. 按 `firmware/system/README.md` 用 `tools/compose-system-tree.py` 组合系统树。
2. `tools/build-system-image.py --source <树> --out <新目录> --generic-flash-size`
3. `tools/system/build-generic-state.py --maintenance-public-key <公钥> --expected-public-sha256 <摘要> --out <新目录>`
4. `RustSimHub-UsbTool-cli.exe pack-create --output <.private 下的新文件> --name ... --version ... --key <.private/flashpack-signing.key> --aboot ... --boot ... --nshstate ... --rootfs ...`
5. `RustSimHub-UsbTool-cli.exe pack-import <包>`

## 刷完以后

1. USB 网卡的地址：20261108 及以后的包由棒子自动分配 `10.55.0.1/29`（`rustsimhub-usb-dhcp.service`，只在 usb0 上，不给网关、DNS 和域名，不开 DNS 服务）。更早的包需要在 Windows 的 USB 网卡上手动设置 `10.55.0.1/29`，不填网关。
2. 首次开机等待提取和初始化完成。
3. 用维护私钥登录，读取设置码：`ssh root@10.55.0.2 cat /var/lib/rustsimhub/rustsimhub-v1/system/first-owner-handoff/setup-code.txt`
4. 浏览器打开 `https://10.55.0.2:8443`（自签名证书，先核对 `cert.pem`），用设置码创建管理员。没有管理员时，这个窗口跨重启一直有效。
5. 在网页或 UsbTool 的“Wi-Fi 重连”里配置 Wi-Fi。

## 目前不包含

- **首次配网热点：** Root 侧的 setup-ap 和转发服务不在系统树里。它们的现有安装器绑定运行中设备的状态，需要刷完后走维护安装。所以新棒子首次配置要用 USB，不能用手机连热点。
- **运营商语音配置：** 没有 `local-volte`，VoLTE 通话可能不可用，要实测。
