# BLE通知路由夹具的接收端关闭前提

固定89488856的正式主线构建在工作区测试阶段失败，尚未进入该轮ARM：`receiver_close_is_not_converted_to_send_success_or_broadcast_fallback` 期望Backend错误却得到Ok。同一158项BLE套件在前几轮通过；失败保留，不盲目重跑取绿。

此测试使用测试专用路径和Candidate UnixDatagram，不是生产org.bluez.AcquireNotify适配。它原来只drop刚经D-Bus传来的一个描述符，就立即断言接收端已不存在。关闭一个副本并不保证不存在其他副本；传输过程可能仍有引用。历史失败没有采样具体持有者，不能断言已找到当时的zbus持有栈。

修正仅在此夹具：显式保留一个接收描述符dup，再对原接收端shutdown(Both)，drop原副本；在dup仍存活时要求发送返回Backend，且没有广播PropertiesChanged。明确制造已关闭接收方向的内核状态，不靠sleep、循环重发或总线清理时序，不把成功入队当作递送成功。测试改名为receiver_shutdown以说明实际前提；生产代码、权限、通知适配、外部接口、成员D任务和验收状态均未改变。

普通用户WSL `cargo test -p rustsimhub-ble --all-features --locked --offline --quiet`：158通过/0失败/0忽略；fmt和全部目标/全部特性严格Clippy通过。新的完整工作区、正式构建和CI需按实际HEAD单独记录。未连接设备，未开BLE或完成其设备验收；81.5%与20/169不变。
