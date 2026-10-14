# 生产首个所有者、短信重启与加密身份恢复

2026-10-14。精确 e927381 源包 SHA256 为
`1db98d42495e57d74c97af9753d201f6f2b38ce3a31378276fe65ab7d33293e6`。
CI 成功，NAS ARM64 workspace 测试和 development/production release 均成功。
新生产程序在 `/opt/rustsimhub/dev-state-p1-e927381`；没有覆盖旧服务。

| 产物 | SHA256 |
|---|---|
| production core，fixtures=false | 23e6e01be8438aa51585414ad8b70d05308b80f851447404600f05edeeb6433d |
| simhubctl | d2f171a9e1aa6df4dd8c48b9032e9a9534977c2d9127a93f320aaf4f13bb8dd2 |
| edge | 7a6e22fa1150d23b67aa2ab4b66391073985148bcacffa09910493c130a3693b |
| update | bd15a3ae2bc71afba2771d81eb98413d78a132b332c0e12e00f95f85dbd09c8e |
| recovery | fafed6bc90f227d81b03f20e0d15939ef9a7a6444ff0f2edca80b0329e323664 |

## 实际通过

- 独立新数据库首次初始化生产 owner，核验设备签名 transcript 后由 NAS 私有客户端钥匙签名。所有者秘密钥匙没有进入棒子。
- 非 Root Core 读取真实 SIM 的 9 条短信。停止/重新启动同一库和身份，等待观察恢复后仍是 9 条，顺序和消息 ID 完全一致；旧 token 拒绝，新签名登录成功。
- 独立离线 recovery 生成 age X25519 接收者。接收者私钥只保留 NAS；棒子只拿公开 recipient，产生身份密文。
- 错私钥、错预期 pin、密文最后字节篡改均拒绝，目标目录仍为空。既有密文输出和恢复目标不能覆盖。
- 先在 NAS 新私有空目录恢复，核验 seed 与原身份哈希一致；再把匹配身份及已停止的数据库副本放进棒子另一个新目录。
- 恢复库启动 production Core，设备 pin 一致，原所有者从只读挂载的钥匙签名登录成功，完整收件箱响应的规范 JSON 哈希与原库一致。原身份哈希保持一致。
- 测试自有临时服务停止，旧四服务 active，PCM 全 closed。本轮无拨号、短信发送、网络配置或普通蜂窝数据激活。

## 修复原因与边界

前一轮只通过加密/错误输入检查，随后原所有者签名失败：旧 CLI 读取 `.client.lock` 仍要求写权限。现在现有身份/客户端 load 使用 O_RDONLY、既有锁和 seed；缺失锁/seed 不创建，pending seed 只由显式 open 完成发布。显式新备份/restore 会创建私有锁。没有改用可写钥匙挂载绕过问题。

短信修复把进程实例和运行时 fence 留在实时安全校验内，把真实 broker UUID/MM owner/SIM 路径作为持久 backend。只从同一已验证 ready line 恢复 source incarnation 与内容哈希；退休、未知重置和 backend/SIM 变化仍隔离。不是按正文合并所有同文短信。

此次仅证明进程重启、离线身份恢复及匹配数据库的授权读取。没有实际断电、整机恢复、恢复入口、恢复后新短信/电话或正式首启网页向导验收；age 仍是独立第三方依赖，完整发布审查尚未完成。

私有证据：`.private/state-p1-e927381-artifacts.json`、`firstboot-owner-e927381.json`、`sealed-identity-e927381.json`。钥匙、目录/设备 pin 和收件箱哈希不进入公共文档。

命令入口为 tools/fetch-verified-artifacts.py、test-firstboot-owner.py、test-sealed-recovery.py。必须传同一份经核验的 artifacts JSON；工具不会通过旧写入方法或旧开发 fixtures 替代生产验收。
