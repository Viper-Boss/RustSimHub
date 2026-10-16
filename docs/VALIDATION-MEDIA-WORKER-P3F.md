# 生产Core与常驻媒体进程实机集成

2026-10-16，最终应用/Native源码 `e271ca15bd8c42d670cd54a7c4e253c97283a053`，归档SHA256 `621994eb8f429519dda1957f96b0a96ec3a8cd8c78c6cdc65b113123bc27354f`。本机194项workspace、19项Native及两者Clippy通过；本机六程序ARM生产构建与NAS固定镜像同源Native构建均退出0。Native为5,451,168字节、SHA256 `43b46708f8f076b7e73d3448fadff1fc72f5985f5670cb5ccedc7e1827a66d84`，Rust1.99.0/ALSA1.2.8，19项ARM Native测试通过，release3分10秒。主程序本机Rust1.98.0编译，分别记录出处，不当作性能对照。

实机Core986和Media984在此前的通用候选RootDirectory运行，实际根目录及exe inode、固定UID和空能力集合已核验。Media只开放固定ALSA节点，Core PrivateDevices=yes；Media控制socket只接受Edge985。两次真实DTLS负向会话检验Root控制拒绝、重复prepare返回同answer、其他会话busy、替换peer拒绝、没有Core票据不打开PCM、关闭后同ID不复活和同一PID复用。随后同一个Media进程承接真实授权会话。

真实呼叫使用新生产数据库和离线签名owner，Root操作目录保持owner私钥。临时最小Voice规则只给Core控制ModemManager电话，其余MM/NM控制仍拒绝。诊断控制端以同源Edge985 CLI在宿主用户态执行，避免为每次状态轮询启动新的systemd命名空间；Core/Media仍使用候选根及上述隔离。这个诊断CLI不能当成正式产品Edge。

NAS只发送编码静音；设备先完成真实DTLS并给出实际peer_key，再由Core验证当前ACTIVE呼叫/租约/owner并消费单次票据，之后才能打开PCM。Core租约预检在实机通过，Media的持续心跳维持许可。所有电话只提交一次10010，提交前独占once标记且先安装18秒独立停止保护；Unknown不自动再拨。没有录音、其他号码、短信发送或普通蜂窝数据测试。

两轮最终源码验收如下，均主动停止媒体并挂断电话：

| 私有报告 | 设备捕获/发送帧 | NAS解码帧/峰值 | 欠载 | 退出检查 |
|---|---:|---:|---:|---|
| media-worker-carrier-fifth.json | 123/123 | 123 / 15687 | 1，可恢复 | passed/cleanup_verified，电话空闲、PCM关闭、Voice重新拒绝 |
| media-worker-carrier-sixth.json | 214/213 | 212 / 15631 | 2，可恢复 | 同上；约4秒持续媒体 |

修正保持原2560帧播放环和320帧周期，只在许可后启动及欠载恢复时预填既有2240帧阈值的静音；实际回读ALSA起停阈值。没有增加缓冲或放宽每秒欠载限制。首轮SDK一秒prepare预算过紧，后改为五秒，读帧/回复及其他命令仍各自有界。私有信令发布竞争改为先写临时文件、设置权限/owner、再原子发布。真实PCM错误和异常断流保留为故障，显式停止才归为正常关闭；耗时日志只含整数，没有声音或凭据。

之前失败与诊断完整保留：carrier-first在拨号前遇私有文件权限竞争，0次电话；second/third各一次电话在4次欠载后保护退出，其中second旧版本错误地把关闭显示为正常，已修正；fourth完成121帧NAS解码，但仍5次欠载，不当成稳定性通过。共新增5次10010，累计20次。所有轮次结束检查通过，临时Core/Media/计时器与Voice规则已停止/撤回，旧四服务active，电话空闲、PCM全关闭。

P3从13增至13.5/20，只计生产Core/常驻Media/受限IPC在候选系统的有界实机集成，整体71.5/100约72%。两轮短测不代表长期稳定；欠载仍需后续压力/故障测试，真实麦克风上行听音、网页信令/播放、来电、多接口、远程/TURN、正式媒体服务及安装配置仍待完成。HTTP仍拒绝直接IssueMedia，`media_available=false`。没有刷写/启动新OS，也没有把旧签名应用包重标为新源码。
