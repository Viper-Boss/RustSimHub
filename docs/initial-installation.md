# 首次安装与可信本地交付

2026-10-16源码检查点：已实现本地初始化与启动守卫；Auth31项、Edge11项及相关Clippy、板级38项通过，完整冻结构建/实机接线尚待完成。当前仍70/100，不把源码和夹具当完成安装。

`rustsimhub-install-initialize MAINTENANCE_PUBLIC_FILE LISTEN TLS_NAME NEW_PRIVATE_HANDOFF_DIRECTORY`由可信本地Root显式调用。要求UFI003持久挂载门禁、固定账户私有目录、应用服务已加载且停止、Root私有交付父目录和合法单行Ed25519维护公钥。先验证完整factory签名及纯语法的监听/TLS请求，再以无cap/PrivateDevices/PrivateNetwork的Core/Edge账户准备各自数据。

Core使用已签名factory的--prepare-device-setup，不打开窗口、不输出设置码。Root保存设备公钥锚点，锚点存在而身份、库或码丢失时拒绝，不生成替代身份。维护公钥必须与既有内容一致，不能通过初始化入口替换权限。SSH主机密钥沿用持久prepare，不启动SSH。

不可变Edge辅助程序新增--check-local LISTEN TLS_NAME和--prepare-local PRIVATE_EDGE_DIR LISTEN TLS_NAME。接受明确私有/回环/链路本地监听，规范化DNS大小写/IP文本。用目录锁、完整pending/installation目录、私有文件同步和NOREPLACE原子发布生成TLS、config和记录；固定Core UID/socket和生产路径。相同请求保留证书，完整pending续作；配置/证书改变、部分状态、额外文件、链接、冲突或不同请求拒绝，不自动修复或修改已安装端点。

交付目录Root0700，六个Root0600文件：device.json、含公钥/证书摘要与origin的edge.json、cert.pem、setup-code.txt、maintenance.pub、完成标记。唯一秘密是一次性设置码，不含设备签名私钥、TLS私钥或客户端私钥；设置码只写文件，终端只报告准备完成。完整交付pending可核验续作，部分内容拒绝。自签名证书与设备pin必须通过可信本地渠道核对，不能用未经验证的网络identity建立信任。

最后写Root控制的installation.complete。缺标记时select/Core/Edge不启动，也不消耗恢复预算。启动检查要求身份、库和TLS/config存在，丢失/损坏停止。Root select仅为读取账户私有元数据保留CAP_DAC_READ_SEARCH，Core/Edge仍空cap；PrivateDevices保持。分区由firstboot在隔离前校验，隔离内仅查挂载和必要文件。

完成后再次调用初始化入口拒绝；应用启动/首次认领是后续明确操作，窗口仍一次性五分钟。已认领、已使用窗口或残缺状态不自动清除标记，不回退维护模式。主应用和不可变安装工具分别冻结记录，不必因工具改动重编同源应用/Native。

本层不写分区，不生成可刷包；实际镜像安装、每设备私有状态准备、恢复及正式网页仍待接线。初始显式端点不是动态USB/Wi-Fi多接口、mDNS或远程relay交付，这些仍在原P4范围内。
