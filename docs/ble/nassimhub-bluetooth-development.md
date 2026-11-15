# NasSimHub 蓝牙开发与安装

本候选依赖 RustSimHub 原有 AirSIM 本机控制和媒体接口。蓝牙棒子面向一个已批准并完成系统配对的 NasSimHub，提供设备状态、读短信、通话记录、通话控制和双向语音。不会建立 IP 网卡或向该连接转发上网流量。

## 棒子端

- 构建并按现有签名安装流程提供 Core、带 `airsim-voble` 的 Edge，以及 `cargo build --locked --release -p rustsimhub-ble --features nas-bluetooth --bin rustsimhub-nas-bluetooth` 产出的蓝牙程序。侧车安装在 `/usr/local/libexec/rustsimhub-nas-bluetooth`。将三个蓝牙 systemd 单元和模式脚本安装到固件覆盖层所示路径。
- 安装时停止两个独立角色的开机启用，只启用 `rustsimhub-bluetooth-mode.service`。由它读取设备已存模式，默认 AirSIM。它通过 Core 的近期 Owner 权限受理控制台的请求，切换期间屏蔽重复提交，通话期间拒绝切换，目标启动失败时恢复原模式。两套配对独立保存。
- 在可信维护终端完成经典蓝牙配对并核对双方数字确认。将批准的 N1 经典蓝牙 MAC 写入 root 管理的 `/etc/rustsimhub/nas-bluetooth.conf`，变量名 `NAS_BLUETOOTH_PEER`。控制台选择「NasSimHub 蓝牙」。打开一次性的应用配对窗口，让 N1 用 `rustsimhub-bluetooth pair` 建立独立应用密钥。
- 两条 RFCOMM 配置使用通道号 23 和 24。业务流最多读取一个受限请求，验证配对/业务选择及路径，再交给现有的签名鉴权；明确拒绝 CONNECT、外部 URL、短信写入和自定义代理。媒体流只转发给现有 Core 媒体授权检查使用的 PCM 套接字，保持 8/16 kHz、单声道、20 ms，单次写入超时就结束当前流。当前实现传 PCM，压缩待真机吞吐测量后评估。

## NasSimHub 端

NasSimHub 改动提供开发候选客户端，复用已启用的 AirSIM 网关和已授权的指定线路对外提供数据和语音。Docker 中只通过 N1 宿主机的两个私有 Unix 套接字访问已配对棒子。当前不自动在原生 Web/Node 设备目录注册蓝牙线路。不增加外向 HTTP 代理、蜂窝默认路由或系统蓝牙网络接口。

## 验证

- Rust AirSIM、BLE、Wire、Edge 共 210 项通过。Core 224 项通过，2 个 AP 首次配网夹具失败，在未修改的 AirSIM 基线也复现相同两项。模式控制用合成服务核对通话时拒绝、拒绝过期页面请求、互斥切换和切换失败回滚。
- Go 客户端、AirSIM 网关适配和辅助命令的本地范围测试覆盖封装配对、签名业务、双向 PCM、不同账号窃取通话拒绝、结果不明不重试。相关 ARM64 侧车构建通过。
- 没有可用于真机验证的 N1。本次未签名部署或修改在线棒子。需要在 N1 上验证 BlueZ 配对、语音延迟及长期通话、断开重连、Docker 权限和双模式切换。
