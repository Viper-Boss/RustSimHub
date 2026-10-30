# 真实 Rust GATT / 跨 UID / 候选策略集成验证

2026-10-31（Asia/Shanghai）。在 #125/#127 的已测组合上，补齐此前“真实Rust”和“不同UID策略”各自通过而未集成的缺口。仅新增显式手动测试，没有改产品执行代码、wire、密码、Root角色或服务启动。

## 实际经过的边界

私有 dbus-daemon UID1001、**产品 GattExport/BluezControl/PeerLease UID1002**、外部合成 BlueZ UID1003、外部调用者 UID1004。四者实际有效cap=0/NoNewPrivs=1。候选policy原字节保持：SHA256 `a878a9852c29b51928823e5b8f914feeb1e87d174c609be736b6e18b4bd7900b`，user名rustsimhub-ble不改，仅由容器只读合成NSS映射测试UID。没有放宽接收/回复/FD规则。

| 路径 | 真实结果 |
|---|---|
| 控制/租约 | 真实broker unique owner，Device1 GetAll/Connected/Adapter及订阅，固定RegisterApplication到hci0 |
| 导出 | ObjectManager恰好1服务/4特征，Auth/Result无Value属性，DeviceInfo字节一致 |
| AcquireNotify拒绝 | 标准路径但错误正文签名、MTU22拒绝；之后合法请求仍成功，未提前消耗FD槽 |
| AcquireNotify正例 | **标准a{sv}请求及hq回复**，Auth/Result各一个真实SCM_RIGHTS FD，SOCK_SEQPACKET/MTU23；重复Acquire拒绝 |
| 通知 | 产品GattExport.notify真实运行；不同UID接收端分别读到Auth/Result完整独立RB1包，不把send Ok当送达证明 |
| 写回 | 标准WriteValue，经产品sender/peer/epoch/队列/前后check，注入host仅核对不透明帧并确认；调用方实际收到空回复 |
| 外部UID | 同一路径/标准AcquireNotify请求被真实broker AccessDenied拒绝 |
| 断连 | 合成外部Device1 Connected=false信号；产品check_peer_current/notify永久StaleContext；对端两个FD实际EOF |
| 清理 | 固定UnregisterApplication实际执行一次，之后产品close_connection；两端测试正常结束 |

这是 **1个编排试验、3个实际libtest角色各1通过/0失败**，不是新增3项普通suite，也不把表格行数冒称独立测试。普通 `cargo test -p rustsimhub-ble --all-features` 仍141通过，不运行这个手动target；没有新增ignore。所有正文由zbus标准类型实际序列化，替代#127头权限试验中的合成字符串/空元数据；#127原9场景也重新验证通过。

## 重现

```sh
source /workspace/toolchains/activate-rustsimhub.sh
python3 -B firmware/system/ble/run_policy_kernel_uids.py --rust-gatt
cargo test -p rustsimhub-ble --all-features --locked --offline
cargo clippy -p rustsimhub-ble --all-targets --all-features --locked --offline -- -D warnings
python3 -B firmware/system/ble/run_policy_kernel_uids.py
python3 -B firmware/system/ble/test_inspect.py -v
cargo fmt --all -- --check
git diff --check
```

启动器新增显式--rust-gatt，构建test=false/required-features的手动Rust target并复制到只读容器输入；默认不触发Rust构建，旧9场景保持。配置信任源、镜像、NSS、daemon及公共库隔离同[策略试验](service-policy-kernel-uids.md)：固定镜像、断网/只读/3项setupcap/角色0cap、32MiBtmpfs/16PID/128MiB/2CPU；宿主/生产NSS及system bus不变。进程间fixture命令走私有stdin管道；每个角色断言实际/proc UID/cap，操作有截止，finally只清理自身子进程及唯一容器。没有mock运行开关或自动Docker路径进入产品。

## 未验收项

外部BlueZ Manager/Device1仍合成，host写确认仅合成完成端点；**不是实际BlueZ/HCI/RF、浏览器、A安全会话、Core恢复权、Wi-Fi事务、生产NSS/角色或实机cgroup预算**。RB1包仍不透明，不能由本试验推导认证成功。没有密码/Root/迁移格式或生产服务修改；没有设备/生产数据库/签名/部署/电话/短信/蜂窝。关闭FD/总线不证明空口清理。

本包补掉D22/D24的组件集成验证缺口，完整步骤新增0，仍21/26离线包已提交、剩D22–26五步，固定P0–P7约80%。下一包继续把实际组件适配到既有GattBackendV1，仍保持SecurePort缺失时拒绝启动，不接管A密码实现。
