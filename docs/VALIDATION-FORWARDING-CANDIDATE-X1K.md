# X1K：冻结源码、整套 ARM64 与既有信任签名候选

2026-10-25。已准备可审查的六文件候选，**未上传、暂存或安装到棒子**。原 P0–P7 仍 80%，X1 扩展单列；本记录不是实机转发验收。

## 来源和构建

应用源码固定 `6853d71f2d45ad56dce609b5953f1681e1de97ea`，其 [CI](https://github.com/Viper-Boss/RustSimHub/actions/runs/40174116864) 两个任务通过。616 个归档文件逐字节核对 Git blob；源码 tar 的 SHA-256 为 `b4e54a564eda7c64ca7513d1e16b7bdaf0924f0831dfef1cf3e579caf8075e0c`。另一个会话的两段未提交文档笔记没有混入归档或提交。

本机离线冻结构建完整通过436项 workspace 测试，产出九份生产 ARM64 程序：Core、simhubctl、Edge、更新器、恢复/迁移工具、灯控、网络 agent、forwarder。各自 ELF/摘要和 glibc 边界核对通过；Core/Edge/forwarder/updater 的 QEMU 元数据均绑定同一源码摘要。运行时开发夹具关闭，主库 schema6、worker 协议4、应用格式1/2保持明确。

同一归档另在 NAS 的既有固定 ARM64 镜像中构建 Native 音频：无网络、只读源码、非Root、空能力、2CPU/1536MiB限额，无设备映射。33项测试和实际 `--build-info` 通过，音频路由能力与源码摘要核对通过。沿用固定离线镜像的 Rust1.99 Native 工具链；workspace仍为1.98，不能说所有部分使用同一工具链。制品和日志已下载、回读摘要核验；只移除了这次精确匹配且成功退出的构建容器，日志/源码/缓存保留。

初次完整构建在默认 WSL 网络中有5项出口用例失败，原日志/exit保留。只连127.0.0.1的对照探针确认：默认环境绑定lo的TCP连接超时，独立回环命名空间、同一UID1000和空能力下连接及前后ifindex均正确。使用此前已通过的隔离测试配置后，同一冻结源码完整通过并完成交叉构建。没有取消绑定检查、增加生产能力或允许默认出口回退。

## 只读设备预检和签名

NAS到USB维护地址的首次SSH超时，记录保留；只读接口/路由/USB和两个已知地址检查确认Wi-Fi维护口仍在线。沿用同一预先固定的设备SSH主机身份，经Wi-Fi核验，没有新增SSH信任或修改NAS网络。

精简固件没有Python；一次只读检查脚本因此失败，未安装解释器。随后使用现有SFTP、systemctl查询及实际已安装更新器的 `verify-package`，核对当前选中组、签名、文件集、升级台账、Core/Edge活动状态和现有应用公钥。本地保留密钥与设备当前信任公钥匹配，现场没有待处理候选。

用这把**现有受信任开发密钥**签出下一epoch47，未生成新密钥或改变设备信任。六文件组总载荷为19,579,833字节，低于24MiB限制，包含同源码的Core、simhubctl、Edge、Native、forwarder及Opus许可证。这是本设备开发信任下的生产特性候选，不是官方生产发行密钥签名。

实际候选再次通过Python/OpenSSL与生产ARM64更新器的QEMU核验，manifest/载荷摘要一致；schema5和损坏worker负例被拒绝。Native的glibc需求也在设备既有边界以内。第一次QEMU核验因从Windows复制来的目录权限被严格拒绝，原失败保留；只将本机临时验证目录规范为0700后通过，未放松更新器权限要求。

## 保存位置与剩余工作

私有索引：`.private/forwarding-source-20261025-first.json`、`forwarding-native-20261025-first.json`、`forwarding-device-preflight-20261025-third.json`、`forwarding-signed-candidate-20261025-first.json`。源码归档、台账细节、公钥信息、候选及私有签名材料均不提交Git。失败尝试保留，最后一次完整构建的成功标记以已终止的对应进程和完整日志核对，不能把首轮遗留exit当成后轮状态。

下一步仍是：一致性备份/回读、UID/GID冲突检查、维护安装及明确的outbox兼容策略，之后才做真实默认OFF、开关、合成递送、撤权/断网和冷启动验收。应用文件组回退不等于业务数据回退；不能为了恢复旧代码而盲目回退短信提交记录。当前Owner、短信、Unknown及恢复预算保持原状；本轮未拨号、未发送短信、未测试普通蜂窝数据，未调用关机驱动。
