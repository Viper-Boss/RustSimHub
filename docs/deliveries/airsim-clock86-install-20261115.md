# 应用86上行时钟补偿与自动纯音诊断 · 2026-11-15

2026-11-15最新实测：86两手机复验：用户确认小卡消失、下行持续正常，但上行有声→静音→恢复反复循环，连续性未通过。本通已结束，64.216秒单一PCM段、服务重启0；软件补静音13/3163、Native丢0，手机输入队列丢35（溢出34/整帧过期1），没有中途换链证据。累计峰值不能解释循环静音；继续补两秒匿名输入/输出及实际ALSA写入电平窗口，不存真实音频，不把纯音成功当真人通过。81.5%、20/169、10.6%保持。

PR247正常合并052b2e1，固定3b3c051/归档448a13b2。纯音复现85跳过式定时器在批量输入与调度停顿下的缺口；86以经过时间累计有限信用、10ms最小间距和30ms首次补充相位恢复节奏，手机输入明确改为200ms/10帧、60ms起步缓冲，原首字节/Native60ms/写40ms及权限保持。61组棒子/57组本机合成测试保留全部旧失败；最新55秒三条件各2750帧连续且样本差值0，85压力对照110/2720静音。1755 Rust/33原忽略、12 ARM/54音频/strict/8网页通过；云端计费零步骤。86验签6拒绝、新85备份回读、受限准入/唯一启动/健康确认/独立复核通过，原短信/配对/Root信任与85回退保留。真实手机连续性仍待复验；合成假声卡不等于iPhone/ALSA/RF验收。81.5%、20/169与10.6%保持。

## 来源、改动与测试

固定源码3b3c051f4913613edce7933aa3a1956081768b67，完整归档448a13b2fe6a1f6ffd198d0e42d3e8288a57db9dbf45be54b0b9a7971d39e18d；合并052b2e14801d931560ef6b28f6b0411cd56d663f与冻结受测树一致。1755 Rust通过/33既有专项忽略、21 PCM、12正式ARM、54 Native ARM/QEMU、fmt/全仓全特性全target严格Clippy和8网页入口通过。云端run43664586182两任务steps=[]，付款/消费上限，未记CI通过。

手机8k/16k输入原首字节年龄与有限FIFO保持，输入预算明确调整到200ms/10帧。计时不再因Skip错过调度窗口就丢发送机会：最多3信用，按20ms经过时间补充、最小10ms写间距，初始60ms蓄帧、首次30ms后补充。Native播放/下行60ms、写40ms、鉴权/租约/RPC限制均未修改。新增确定性信用/蓄帧测试，既有分片两采样率测试扩展到180ms，并在实际旧84运行同一行为反例确认失败。

## 纯音证据及限制

共61组棒子、57组本机完成样本，使用997Hz合成音、真实Edge生产函数/Native分帧/Pump与假声卡，经隔离回环TCP/Unix。均匀20ms、88ms批量及每500ms注入35ms反应器停顿三条件分别记录。最新55秒每候选组2750帧全部收到并连续播放，稳定段0/2720静音、Native丢0；每组880000样本顺序一致，最大差值0。实际85同条件110/2720静音。20秒电脑和棒子重复也各1000帧完整、差值0。

这不是iPhone麦克风、真实ALSA或射频的端到端测量，也没有采集真实手机到达时间序列，不能据此认定全部根因已穷尽。未录真实通话，未自动拨号。诊断进程实际UID65534、CapEff0、NoNewPrivileges、PrivateNetwork/PrivateDevices、生产数据路径不可见，前后85账本和服务一致。

旧失败实验全部保留：简单Burst仍缺口；无节奏的3帧补发使Native过期丢帧；160ms/6容量溢出；初始2信用在20秒压力样本仍有单个缺口。实验94的60秒扩展在合成Authority固定60000ms到期失败，没有完成候选样本；未改生产权限，改55秒后实验95完成。进程正常退出与音频通过分开判定。

## 安装和数据

86清单0b291799f88e8273886e5d04ebc48f078a2294a00e3594bc780fdcddfec7404d，六载荷24019441字节，现有开发信任签名。真实ARM更新器验签及六类拒绝、12恢复和10配对保护场景通过。暂停后重新只读核对85基线，新备份回读/SQLite、受限admit、唯一start、健康confirm与独立只读复核完成。confirmed86/floor86，无candidate/attempted，恢复timer停止。

原130短信、Owner与手机配对保持，Root助手/正式更新器/服务单元/信任未改，85回退与新备份保留。音频7/7路由及实际8个HTTPS资源检查通过，普通数据关闭、转发PID0，无重启/NAS/分区/PMIC操作。只清理逐文件核验且本机有可读同字节副本的旧78/85候选暂存与已结束诊断程序，共72119534字节，Root空间140016→210628KiB；不删除已装版本、真实备份或恢复材料，不降低128MiB门槛。

## 待验收

真实两手机双向、连续多通/长稳、不同网络/断网/撤权/争用继续验证。纯音通过不记真实用户通过。原P0–P7验收81.5%、20/169、加权10.6%不变，其他成员任务和主仓11项原dirty保留。

## 全部完成的棒子样本

### 实验85

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|1/370|400/0|
|actual85_batch88|3/370|397/0|
|actual85_batch88_stall35|15/370|385/0|
|burst_hypothesis_smooth|0/370|400/0|
|burst_hypothesis_batch88|0/370|400/0|
|burst_hypothesis_batch88_stall35|7/370|393/0|

### 实验86

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|0/370|400/0|
|actual85_batch88|6/370|394/0|
|actual85_batch88_stall35|13/370|387/0|
|burst_hypothesis_smooth|0/370|400/0|
|burst_hypothesis_batch88|0/370|400/0|
|burst_hypothesis_batch88_stall35|9/370|391/0|
|bounded_hypothesis_smooth|0/370|400/1|
|bounded_hypothesis_batch88|33/370|400/35|
|bounded_hypothesis_batch88_stall35|7/370|400/8|

### 实验87

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|1/370|400/0|
|actual85_batch88|2/370|397/0|
|actual85_batch88_stall35|17/370|383/0|
|burst_hypothesis_smooth|0/370|400/0|
|burst_hypothesis_batch88|0/370|400/0|
|burst_hypothesis_batch88_stall35|11/370|390/0|
|bounded_hypothesis_smooth|0/370|400/1|
|bounded_hypothesis_batch88|0/370|400/0|
|bounded_hypothesis_batch88_stall35|11/370|390/0|

### 实验88

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|0/370|400/0|
|actual85_batch88|3/370|396/0|
|actual85_batch88_stall35|17/370|383/0|
|burst_hypothesis_smooth|0/370|400/0|
|burst_hypothesis_batch88|0/370|400/0|
|burst_hypothesis_batch88_stall35|9/370|391/0|
|bounded_hypothesis_smooth|0/370|400/0|
|bounded_hypothesis_batch88|0/370|399/0|
|bounded_hypothesis_batch88_stall35|11/370|388/0|

### 实验89

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|1/370|400/0|
|actual85_batch88|3/370|397/0|
|actual85_batch88_stall35|15/370|385/0|
|bounded_hypothesis_smooth|0/370|400/0|
|bounded_hypothesis_batch88|0/370|400/0|
|bounded_hypothesis_batch88_stall35|7/370|394/0|

### 实验90

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_smooth|0/370|400/0|
|actual85_batch88|0/370|400/0|
|actual85_batch88_stall35|17/370|383/0|
|bounded_hypothesis_smooth|0/370|400/1|
|bounded_hypothesis_batch88|0/370|400/0|
|bounded_hypothesis_batch88_stall35|0/370|400/0|

### 实验91

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_batch88_stall35|42/970|957/0|
|bounded_hypothesis_smooth|0/970|1000/0|
|bounded_hypothesis_batch88|0/970|1000/0|
|bounded_hypothesis_batch88_stall35|1/970|1000/0|

### 实验92

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_batch88_stall35|43/970|957/0|
|bounded_hypothesis_smooth|0/970|1000/0|
|bounded_hypothesis_batch88|0/970|1000/0|
|bounded_hypothesis_batch88_stall35|1/970|1000/0|

### 实验93

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_batch88_stall35|41/970|959/0|
|bounded_hypothesis_smooth|0/970|1000/0|
|bounded_hypothesis_batch88|0/970|1000/0|
|bounded_hypothesis_batch88_stall35|0/970|1000/0|

### 实验95

|策略与条件|稳定段静音|Native收到/丢帧|
|---|---:|---:|
|actual85_batch88_stall35|110/2720|2640/0|
|bounded_hypothesis_smooth|0/2720|2750/0|
|bounded_hypothesis_batch88|0/2720|2750/0|
|bounded_hypothesis_batch88_stall35|0/2720|2750/0|
