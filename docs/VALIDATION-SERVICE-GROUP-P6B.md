# 签名应用服务组和本地健康确认

2026-10-14，tools/test-service-group.py 第三轮实机验收通过。
Core/CLI/Edge/Update 为 e927381 的已核验生产产物，来源及哈希见 VALIDATION-STATE-RECOVERY-P1C.md；原生媒体沿用 93a4005 的已核验 ad18aab8… 产物。签名 manifest 的 source 字段代表主 workspace 源包，不宣称沿用的媒体由同次编译得到。

## 已验证

- NAS root 私有开发发布钥匙签三个应用包，钥匙不进入设备。三个 epoch 使用相同程序，测试的是选择器/服务组生命周期。
- 设备在新的 Root 私有 store 验证签名与 payload 后选择版本。业务 UID 通过私有只读挂载看到同一版本，不可写 selector state 或程序。
- Core 986、Edge 985 真实运行；核验 `/proc` 四个 UID 和 CapEff/CapPrm/CapAmb/CapBnd 全零，核验进程挂载视图中实际程序哈希。
- Core 不能读取 Edge TLS 或本地健康客户秘密钥匙；Edge 不能读取 Core 身份或健康客户钥匙。实际在对应 mount namespace 下用业务账户验证拒绝。
- 原 owner 只在首次离线授权健康客户时从 NAS 签名。健康客户只有 `status.read`，健康检查期间设备本地完成 pin 验证/挑战签名/登录/status/line ready，无 NAS 请求；短信读取与所有者配对管理均拒绝。
- HTTPS 通过 USB 本地地址固定端口，只用受信任本地 CA 和 IP 验证访问。页面 200；没有普通蜂窝数据激活或隐式 fallback。
- factory 健康通过后确认。候选实际 Core SIGKILL、停止整个自有组，再选择回到 factory；原有本地健康授权可用。
- 第三个版本健康后确认，停止/重新启动选择仍是该版本，本地健康及 HTTPS 再次通过。
- 有限运行时与退出清理有效，自己的服务停止，旧四服务 active，PCM closed。本轮拨号/短信发送均为零。

## 两次已保留失败及修正

第一次 `systemd-run` 提前检查了尚未挂载的虚拟 executable 路径。使用已存在的 `/usr/bin/env -i` 作为有限 launcher，让固定、已验证程序在私有挂载建立后执行，不解析用户 shell 字符串。

第二次 signed store 在 umask 077 下产生 Root-only 执行位，非 Root 无法执行。仅对重新验证后选中的固定公开程序设置 0555，版本目录 0755；store 祖先仍 Root-private，业务挂载只读。没有修改身份/配置/状态的私有模式。

## 尚未完成

这里只启动 Core/HTTPS 和有限一次健康检查；没有开启完整媒体组或电话写权限、没有安装/enable 开机单元、没有自动远程下载/在线健康自动确认、更没有断电/完整固件恢复验收。当前控制模式是只读 call-observing，不对外宣称全功能固件可刷。

私有证据 service-group-e927381-first/second/third.json、group-failure-journal.log；第三轮 passed 和 cleanup_verified 为 true。脚本创建新私有目录、短时单元，不复用旧业务库，也不重复未确认 factory 的首次选择。
