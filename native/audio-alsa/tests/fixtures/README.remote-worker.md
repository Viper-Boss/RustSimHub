# 显式远程 worker / 原 Core 媒体协议 / UID 夹具

普通cargo test不会启动两个test=false私有TLS目标，也不会创建账户或切换UID。
首先按README.turn.md准备固定私有Coturn镜像（网络构建只在该容器，遵守会话proxy/CA）。
本目标的合成NSS层不访问网络，UID只存在于临时镜像/容器，不改宿主/设备NSS。

```sh
source /workspace/toolchains/activate-rustsimhub.sh
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/run_private_remote_worker.py --build-fixture
# 已有镜像则不加 --build-fixture；实际image ID及产物/日志hash写work/remote-worker-final/result.json。
CARGO_INCREMENTAL=0 python3 native/audio-alsa/tests/replay_clock_negative.py
```

当前完整私有目标38通过/0失败/1显式ignored。38包含23个复用SDK函数、5个源码内unit、10个新worker目标用例，不把它与标准suite/另一架构相加算新测试。ignored只有worker_actual_uid_role：同一runner随后显式--ignored --exact，分别按真实UID984/985/986执行正例，另外按984/986/987执行非法来源负例；非测试native CLI另实际运行，不把未运行项算成功。

Host共128MiB/16pids/2CPU、read-only、network=none。普通矩阵UID1000/caps空；UID Root控制器只持SETUID/SETGID/CHOWN/FOWNER，无CAP_KILL；三个演员/非法来源演员的实际CapEff为空。Root清理helper先降成子进程同UID。独立Coturn/正常TLS/实际UDP/ICE/DTLS/Opus、真实socket与原Authority/Client、v2 actor/线程均运行；仅业务Context、PCM、私有登记/合成CA是外部后端替代。默认非测试binary没有serve_fixture或loopback登记开关。

`replay_clock_negative.py`只修改git archive导出的私有源码，把新live-clock入口的I/O后取时移除，保留真实production模块/原回归断言；预期返回101且仅该回归失败。原工作树不变，失败hash保留work，不把人为故障注入写成旧生产漏洞。正常实现通过的证据来自同源整仓测试。

ARM使用README.turn.md的既有交叉镜像/工具链：干净tracked git archive，Rust1.98、aarch64-unknown-linux-gnu release、GCC Cortex-A53和实际ARM ALSA/Opus，显式构建remote_worker_private目标。输出真实AArch64 ELF，运行`qemu-aarch64 -cpu cortex-a53 -L / /input/worker-test --nocapture --test-threads=1`；无网络/read-only/UID1000/caps空/128MiB/16pids/2CPU。ARM38/0/1的1项UID角色仅另在x86容器执行，不假称ARM三角色或目标板/真实PCM验收。具体实际SHA/镜像/命令边界见docs/remote/member-d-sec06-evidence.json与交付记录。

nonce定时夹具保持产品期限、真实438/Refresh success/后续UDP与601秒过期拒绝。实际控制请求未排空或重试尚受固定库50ms cadence约束时不能推进30s测试时钟；cfg(test)的真实请求/响应观察+100ms无I/O/无待响应证明只用于稳定这一前提，不接生产授权/解析或任意时间设置。
