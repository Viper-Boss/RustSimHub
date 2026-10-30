# 成员 D：真实 UID / D-Bus / FD 策略手动验证

仓库 Viper-Boss/RustSimHub，分支 `codex/gtw-ble-policy-kernel-uids-20261030`，目标 main，草稿依赖 #126。基线 #126 `2b69cf7594db318e181927c13e3e6dcb46491e33`。代码/全部正式验证 HEAD **`d2f00f0b8d78133c3c5c90fa96f4392c76c55bb7`**，tracked_dirty=false；后续仅本人说明/报告文档。

新增两个显式手动测试入口文件；#126 四份材料、现有只读检查器和Rust/依赖/默认启动行为没有改动。实际容器UID1001/1002/1003/1004，零有效cap/no-new-privileges；生产名称的原policy不改，映射只来自只读容器合成NSS。真实dbus-daemon/libdbus/SCM_RIGHTS证明单FDunique owner回复可达、双FD/输入FD/错误对象/方法/外部UID/全局Value signal拒绝，固定元数据信号通过。

正式命令：

```sh
python3 -B firmware/system/ble/run_policy_kernel_uids.py
python3 -B firmware/system/ble/test_inspect.py -v
git diff --check
```

**手动1个试验/9组断言通过；普通检查16通过、0失败、0跳过；退出码0。** 不是9项独立Rust测试，未运行/虚构本分支整仓950；无新ignore或自动Docker启动。受控放宽两项实验在同一受测代码分别失败（hh/PropertiesChanged），之后模板恢复并重跑正式试验通过。

可核对摘要（完整逐输入摘要由启动器写入本地 result.json）：

| 证据 | SHA-256 |
|---|---|
| policy字节，与#126相同 | a878a9852c29b51928823e5b8f914feeb1e87d174c609be736b6e18b4bd7900b |
| fixture.py | 406f66e330ed0379dadae82f2d562c356c6466b172c4b2d7f266dfcdd4e8e54b |
| dbus-daemon | 32c90f7766d01081f0f052ef8a2879214a69fb8355d5034a07a8ce54c8e8e320 |
| libdbus-1.so.3 | f8c5475204730fa615547b10b642d7e27e5954ed1c12c47507743f2b486ba5a0 |
| 正式container.log | 21ee72e75feb05fcecdda9a783fa2f472f93aef80345dcd5621f91d8e30cddc5 |
| 正式result.json | ba01eac9f58eaf82f83f4d6d6649badd8746f5af95c26d77fb143bbbb206890b |
| 放宽两FD的失败日志 | 6a48e0888452b98baa0fbb8682f3c3f5d57ff2c68f349ee137751330555661ac |
| 放宽Value signal的失败日志 | fd267e255d199dc6c123fbf820d2834e04178f19101a937665ab8c2dbdfe8be7 |

PID/临时地址随重跑变化，日志摘要可变。平台固定镜像、宿主公共库逐字hash；没有复制到仓库或新增生产依赖。AGPL-3.0-only/Codex辅助；#126历史原作者Viper-Boss继续保留，无工时虚报。

[逐场景、命令与隔离/未验收项](../ble/service-policy-kernel-uids.md)。没有宿主NSS/system bus/生产资料、实际BlueZ/HCI/RF、标准正文/Rust跨UID集成、A密码、设备/ARM或Root角色/安装/预算验收，没有部署/签名/短信/拨号/流量。测试对平台Docker使用[cloud-environment-runtime](skill://plugin_connector_1p_c5b7d5df5d7081918f2c4be5a633ed5d/cloud-environment-runtime/SKILL.md)的本地daemon/网络/CA隔离指导；不改环境配置。

新增完整步骤0，仍 **21/26离线包已提交、剩D22–26五步，固定P0–P7约80%**；不把实验或待审角色计为验收。不修改主线全局进度、其他分支、生产Root格式。最终PR HEAD/CI按回读记录，账户限制不绕过。
