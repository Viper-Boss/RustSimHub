# SMS 发送验证（P2）

2026-10-13。本轮新增显式写模式、v4 持久发送许可和 CLI；最终验证进行中。

私有 D-Bus 模拟使用 UnixStream peer，不连接系统 bus；计划核验正常发送、Create 错误、Send 错误/超时、
创建后 SIM 换卡/客户端撤销、ACK 后仍发送中、路径删除重用。存储覆盖单次许可、scope、generation、事务失败和重启 Unknown。

最终 ARM64 / GitHub CI / 一次运营商余额查询结果将在完成后补充；不得将此记录当作已发送证明。
