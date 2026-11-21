# 活跃安装统计参与授权

本段接线只保存和核对“是否允许参与上报”。它不等于真实签到、签名、交易提交或后台服务已运行，也不是 Root 的永久网络授权。

## 入口与默认行为

- 网页在原统计区域增加“是否参与安装统计”。默认不勾选，加载/登录/重绘不会读写授权或上报。
- “保存参与授权（暂不上报）”自动进行原 Owner 登录、读取当前代次、显示明确的参与/关闭确认，持久化本机防重记录后只提交一次。它不会调用 `set_nkn_enabled`、查询统计、生成密钥、签名或提交交易。
- `usage_participation_status {token}`：仅当前 Owner 的 `system.manage` 读取，缺失配置为关闭且不创建文件。
- `usage_participation_set {token,enabled,disclosure_version,expected_generation,expected_core_instance,request_id}`：修改需要原最近五分钟**本地** Owner。开启必须确认声明版本1；关闭必须不带声明版本。generation使用规范十进制字符串，Core instance与请求UUID严格核对。
- `usage_participation_request {token,request_id}`：只查询原请求，重启后用当前合法 Owner 查回原 Core instance 和原保存代次。没查到不能证明未执行，不能自动重发。
- 状态明确返回 `reporting_available:false`，运行状态为 `disabled`、`authorized_not_connected` 或 `reauthorization_required`。保存同意不绕过后续实际签名/发送准入；未来接通实际提交仍需按提交前置、当前权限与明确授权执行，不能把此文件当永久 Root capability。

## 持久与隔离

Core 原服务将已有私有业务状态目录注入提供者，路径固定为其下的 `usage-participation`；网页不能指定目录、密钥或网络地址。构造提供者不会访问文件或启动任务。

新库 `store::participation::Ledger` 复用原统计文件后端的安全目录、单写者锁及原子落盘，不修改旧 `FileStore` 的身份/待确认交易格式。记录绑定产品域、Core公钥、业务数据库UUID、授权代次、关闭代次和最多64条精确回执。Owner公钥/代次只留私有记录，对网页只投影必要状态。

同一请求和输入幂等返回旧回执，不重新应用旧ON。ID冲突、旧CAS代次、错误Core上下文、跨realm/Core密钥、撤销后的Owner代次、格式/文件错误均拒绝。回执淘汰后仍保留单调代次，不能用旧请求重放。修改发布后重新打开实际目录并核对原回执，再返回saved；文件错误可能发生在发布之后，因此不能一概解释为“肯定未保存”。

关闭会持久增加关闭代次、保留历史回执；独立身份、已有待确认交易摘要和缓存逐字节保持。尚无上报驱动，不能把此测试说成已验证在途链上响应取消或设备PID退出。Owner变更后的旧允许记录不能作为当前Owner授权。

Linux复用0700目录/0600文件、有效UID、普通单硬链接文件、逐级不跟随链接、锁inode核对和文件/目录同步。Windows仅继承原后端ACL限制，本轮未运行其生产安全验收。首次创建中断留下不完整目录时显式拒绝并保留现场，不静默重建或清空。

## 浏览器恢复

使用单独的 IndexedDB pending/receipts 命名空间；提交前精确CAS并回读。双击和登录中的并发点击只预留一次；失败不自动重发。退出/离页/重置后的旧回复不能改变当前页面或自动再次提交。查到saved后先保存归档再清理pending；不确定回执只能经用户明确确认保留归档并解除本页阻止，不能冒称已撤销设备授权。归档上限64，优先淘汰已确认项，不能挤掉不确定记录。

蓝牙、AirSIM、音频、Root角色/策略和设备均未修改。
