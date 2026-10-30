# 隔离容器中的真实不同 UID 与 Core owned-channel 验证

2026-10-30。本开发环境宿主为UID1000、无sudo/newuidmap；它提供可用的本地Docker。
本次使用隔离容器的真实进程凭据补上此前缺失的正例，不改变宿主/生产NSS、UID/GID、
系统服务、权限或发布配置。不是把一个UID映射/字符串自报成另一个角色。

## 执行边界

手动测试target `recovery_kernel_uid`，`test=false`且要求两项recovery feature。
普通workspace测试不会启动Docker或切UID，没有新增ignore或默默跳过的通过项。
它仍编译为cfg(test)，合成验证器不会进入Core生产代码/默认构造器。
`--test recovery_kernel_uid`是显式选择隔离target，应通过下面启动器执行。
全目标Clippy只编译该target，不运行它。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
# 首次需要镜像时显式 --pull；已缓存时省略该参数。
python3 crates/core/tests/run_recovery_kernel_uids.py --pull
```

启动器只选 `unix:///var/run/docker.sock`，清除可能重定向的Docker endpoint/context/
TLS选择器，保留DOCKER_CONFIG/注册表认证/平台代理；不启动或重启daemon。
固定镜像 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`。
实际运行断网、只读rootfs、只读挂载测试二进制/脚本、独立tmpfs；no-new-privileges，
32PID/256MiB/2CPU预算，drop全部capabilities后仅setup需SETUID/SETGID/CHOWN/FOWNER。
每次唯一容器名，成功/失败/超时都只清理自己的容器，不挂Docker socket进容器。

setup只在容器内创建临时目录owner1001/group1002/mode2750，再由真实UID1001/GID1001、
有效capabilities=0的Core进程创建UID1001/GID1002/mode0660的Unix socket。
设置SGID时setup临时加入测试GID后立即清空，避免内核清掉SGID位；未增加CAP_FSETID。
所有peer进程由setuid实际切换，使用同一测试GID1002以通过文件系统门禁，
让拒绝正例确实落在Core SO_PEERCRED检查，而非仅被目录权限挡在外面。
这些数值仅为本次容器fixture，不是生产角色选择/预留或NSS验证。

## 实际断言

Rust测试调用真实 SourceGuard、FramedIo、RecoveryChannel、Auth/Store、独立worker：

- 实际Root0、Core自身1001、network1003、edge1004、forwarder1005及未知1006都拒绝，
  即使同测试socket组且载荷声称uid=1002/verified=true/channel=ble，也不能造PeerContext。
- 真实BLE1002通过来源检查，但无注入验证器时attach仍拒绝；UID不是恢复授权。
- 注入**cfg(test)合成外部验证器**后，实际接受的内核上下文nonce进入同Core Auth绑定，
  已签名测试grant与请求门禁产生ReadStatus能力；验证调用确实在owned worker中完成。
- Core向UID1002实际写回固定测试握手帧，接收端逐字节核对真实长度/版本/kind/载荷。
- 同UID1002第二连接在占槽时拒绝，不能替换原channel或使原能力失效。
- Drop channel使旧能力失效，下一次真实连接取得不同nonce；旧能力不能继承，
  新能力仅本连接有效；真实CloseTransport写回在Drop前已使能力失效，接收端核对终止帧。

这是**1项手动测试、13组场景断言**，不是11项测试。
返回的result.json记录精确源码HEAD、tracked_dirty、固定镜像及binary/runner/log SHA256。
默认整仓计数与这项手动结果分开，不能把未自动执行的target当自动通过。

## 不代表什么

fixture用公开固定Ed25519测试材料建立临时schema7/Owner/独立grant；
外部请求验证器合成，仅识别测试marker并生成绑定输出，不是A的实际握手/AEAD/防重放
互操作，不选择密码suite。没有Wi-Fi/电话/短信或其他业务派发、生产数据库/设备/NAS，
没有签名发布/部署、目标NSS/Root材料或ARM设备验收。
新增证据补上“实际不同UID的内核来源及owned-channel门禁”，仍不等于D22完整验收。
缺A端口的生产默认继续拒绝启动完整恢复服务；本fixture不给任意payload授权。
