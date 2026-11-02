# 显式远程 worker / 原 Core 媒体协议 / UID 夹具

普通cargo test不会启动两个test=false私有TLS目标，也不会创建账户或切换UID。
首先按README.turn.md准备固定私有Coturn镜像（网络构建只在该容器，遵守会话proxy/CA）。
本目标的合成NSS层不访问网络，UID只存在于临时镜像/容器，不改宿主/设备NSS。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/run_private_remote_worker.py --build-fixture
# 已有镜像则不加 --build-fixture；实际image ID及产物/日志hash写work/remote-worker-final/result.json。
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/replay_clock_negative.py
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/replay_normal_host_negative.py
```

当前完整私有目标38通过/0失败/2显式ignored。38包含23个复用SDK函数、5个源码内unit、10个新worker目标用例，不把它与标准suite/另一架构相加算新测试。worker_actual_uid_role和normal_host::actual_uid_edge_normal_host由同一runner随后分别显式--ignored --exact执行。旧UID夹具按真实UID984/985/986执行正例，另外按984/986/987执行非法来源负例，非测试native CLI另实际运行。新增正常Core夹具以UID986运行原Auth/Store/Calls Controller/normal dispatcher/media_ipc，以UID985调用非测试Host API，以UID984运行真实worker；正常身份/原租约→真实TURN/双向Opus→原Owner撤权关闭媒体均验证，不预造媒体票据。

x86每个容器共128MiB/16pids/2CPU、read-only、network=none。普通矩阵UID1000/caps空；UID Root控制器只持SETUID/SETGID/CHOWN/FOWNER，无CAP_KILL；三个演员/非法来源演员的实际CapEff为空。Root清理helper先降成子进程同UID。独立Coturn/正常TLS/实际UDP/ICE/DTLS/Opus、真实socket与原Authority/Client、v2 actor/线程均运行。旧worker夹具仍用合成业务Context，新增正常Core夹具不替代原Auth/Controller/Authority，只替代外部私有ModemManager/PCM/登记/CA/远端音频对端。默认非测试binary没有serve_fixture或loopback登记开关。

`replay_clock_negative.py`只修改git archive导出的私有源码，把新live-clock入口的I/O后取时移除，保留真实production模块/原回归断言；预期返回101且仅该回归失败。原工作树不变，失败hash保留work，不把人为故障注入写成旧生产漏洞。正常实现通过的证据来自同源整仓测试。

ARM使用README.turn.md的既有交叉镜像/工具链：干净tracked git archive，Rust1.98、aarch64-unknown-linux-gnu release、GCC Cortex-A53和实际ARM ALSA/Opus，显式构建remote_worker_private目标。输出真实AArch64 ELF，运行`qemu-aarch64 -cpu cortex-a53 -L / /input/worker-test --nocapture --test-threads=1`；无网络/read-only/UID1000/caps空/128MiB/16pids/2CPU，同一目标38/0/2。

另将原Core x86测试ELF、ARM worker测试ELF与normal_remote_uids.py分别只读放入/input/core-test、/input/worker-test、/input/runner.py，使用Dockerfile.worker-uids在既有QEMU运行镜像上创建临时NSS层。Root组合容器仍network=none/read-only/128MiB/2CPU，只授上述4项cap；/tmp和/fixture各64MiB tmpfs，/fixture mode1777，/run为8MiB。设置RUSTSIMHUB_PRIVATE_TURN_FIXTURE=1、RUSTSIMHUB_PRIVATE_UID_FIXTURE=1、RUSTSIMHUB_ARM_ACTORS=1，执行python3 /input/runner.py。该测试only开关只让私有runner为Media/Edge两个演员添加QEMU前缀，不改变library或安装配置。**组合夹具需24pids**：原16pids实际失败，Coturn无法创建线程；单worker矩阵保留16pids，未改产品预算。原Core运行在x86，不能称完整ARM Core/整板或真实PCM验收。

ARM矩阵与另一QEMU容器并行时实际有过37/1/2（旧self_check返回Bounds，具体分支未定位）；同一ELF在原预算下独立串行重验38/0/2。失败日志/来源一并保留，未放宽媒体期限或断言，不把重跑当负载稳定性验收。[最新逐SHA/镜像/hash证据](../../../../docs/remote/member-d-normal-host-evidence.json)；[原SEC-06历史证据](../../../../docs/remote/member-d-sec06-evidence.json)保持对应旧来源。

nonce定时夹具保持产品期限、真实438/Refresh success/后续UDP与601秒过期拒绝。实际控制请求未排空或重试尚受固定库50ms cadence约束时不能推进30s测试时钟；cfg(test)的真实请求/响应观察+100ms无I/O/无待响应证明只用于稳定这一前提，不接生产授权/解析或任意时间设置。
