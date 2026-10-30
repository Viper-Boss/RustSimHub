# D24：真实不同 UID 的私有 D-Bus 策略验证

这是 #126 候选权限的手动隔离测试，不安装角色或服务。四个真实 UID：dbus-daemon 1001、BLE 1002、合成 BlueZ 1003、外部调用者 1004；各进程实际有效 capabilities=0、NoNewPrivs=1。真实 EXTERNAL 认证/内核 UID、dbus-daemon 策略、libdbus 编码/发送及 SCM_RIGHTS，不使用 payload 自报来源。

候选 `policy user="rustsimhub-ble"` **逐字不改**，容器只读合成 passwd/group 将该名映射到1002。另两名 fixture 用户单独许可发送/接收，唯有合成 BlueZ 可持有 org.bluez；这些测试许可不进入候选模板。宿主 NSS、system bus 和生产设备不变。最初仅数字 user、没有 NSS 条目的试验实际失败；补合成容器 NSS 后才运行正例，没有放宽 BLE 策略绕过。

## 实际执行的 9 组断言

| 场景 | 实际观察 |
|---|---|
| BlueZ owner 向固定 AuthExchange AcquireNotify，BLE 向该 unique name 回复一个 FD | h 回复通过，接收端从真实 SOCK_SEQPACKET 读到 POLICY_UID_FD；证明 well-known 策略匹配当前 unique owner |
| 同一被请求回复附两个 FD | dbus-daemon 拒绝，调用方 NoReply，未把 send/flush 成功冒充送达 |
| 不相关对象路径 | AccessDenied |
| DeviceInfo 上 AcquireNotify | AccessDenied |
| AuthExchange 上 AcquireWrite | AccessDenied |
| BlueZ 输入方法携一个 FD | AccessDenied；输入 max_fds=0 生效 |
| 全局 PropertiesChanged signal | 已订阅 BlueZ 连接未收到，固定请求回复作为此前发送的有序屏障 |
| 固定根 ObjectManager InterfacesAdded signal | 已订阅 BlueZ 连接收到 |
| 外部 UID 调用相同 AcquireNotify | AccessDenied；拥有不同身份的连接不能冒充 org.bluez |

**这是 1 个手动试验、9 组场景断言，不冒称9个独立Rust测试。** 候选 policy SHA-256 `a878a9852c29b51928823e5b8f914feeb1e87d174c609be736b6e18b4bd7900b` 与 #126 相同；没有修改许可来获得成功。

另外两次受控失败检查均实际执行：只把 method_return max_fds 从1改2，原断言失败并观测到 hh；只追加 PropertiesChanged signal allow，原“不收到”断言失败并观测到 PropertiesChanged。之后恢复模板原字节，正式9组及既有16项检查全部重跑通过。受控变体只用于实验，不提交到模板。

## 重现和隔离

```sh
python3 -B firmware/system/ble/run_policy_kernel_uids.py
python3 -B firmware/system/ble/test_inspect.py -v
```

启动器是显式手动入口，普通检查不启动 Docker。只选择本地 Docker socket，清端点/context/TLS选择变量，保留平台 proxy/认证/CA配置；镜像固定为 `python@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb`，默认 pull=never，可显式 --pull。本次已缓存，无网络拉取。容器 network=none、只读 rootfs/输入/NSS、32MiB独立tmpfs、16PID/128MiB/2CPU、no-new-privileges；setup只有 SETUID/SETGID/CHOWN，降权后的 daemon/三个角色均0cap。

Linux x86_64 宿主固定 `/usr/bin/dbus-daemon` 及其 ldd 解析出的实际公共系统库拷贝到本地只读输入，通过所拷贝 loader 运行；不是任意可执行文件参数，也不读取宿主 passwd/group/凭据。宿主库版本可变，每个输入都在 result.json 记录SHA-256；单有镜像digest不代表宿主库被固定。二进制/库只留本地 work，不入 Git。libdbus 的 ctypes C ABI 代码只在手动测试进程；不作为产品 GATT/密码实现或新增生产依赖。

setup 创建临时目录/配置后让给 daemon UID；退出用同 UID 的有限子进程给自身 fixture 进程发信号，不加 CAP_KILL。所有执行有截止，finally 清理唯一自身容器/进程，并恢复自身临时目录所有权以释放。不重启 Docker daemon、不使用 privileged/host network、不修改生产权限或选择 UID982。

## 证据边界

测试方法的正文故意使用合成字符串，元数据 signal 使用合成空体，隔离验证**消息头权限、身份和 FD 数量**。这不是实际 BlueZ、标准 AcquireNotify a{sv}/hq 正文、Rust GATT 服务、A 安全会话、设备/NSS/Root 安装或 RF/ARM/cgroup预算验收。真实 Rust 的 FD/peer 测试在 #122；不能把两组各自通过等同于它们已在两个 UID 间集成。生产组合系统策略、role/UID、实际 daemon/host 和缺硬件/停止清理仍待审核/集成。

原来的单 UID broker 16项结果保持；本试验补掉其中“未跨 UID/未真实FD许可”的部分缺口，不修改原报告的历史命令或声称 D24 全部完成。仍21/26离线包已提交、剩5步，固定P0–P7约80%。
