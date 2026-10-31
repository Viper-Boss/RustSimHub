# A-11：公开依赖事实清单工具

工具tools/dependencies/inventory.py只盘点公开锁文件与包声明，不作许可证兼容性、漏洞或生产依赖批准。默认仅打印计划；显式--run才执行两个固定cargo metadata --locked --offline --all-features命令，不构建、不下载、不执行Go/NKN或设备命令。每命令30秒，输入/输出每项≤4MiB，包数≤2000；拒绝链接、越界、非普通/超大文件、重复锁包或无效registry校验和。

```sh
python3 tools/dependencies/inventory.py
python3 tools/dependencies/inventory.py --run --upstream /explicit/fixed-public-nknguard --output /tmp/new-inventory.json
python3 -m unittest discover -s tools/dependencies -p 'test_*.py' -v
```

输出必须是新文件。实际盘点完成但缺少系统/许可/通告证据时退出2，completeness=incomplete、approved=false；这不是“扫描全通过”。默认计划退出0，不执行metadata或读取上游。失败stderr不会被附到公开JSON，避免导出Cargo缓存绝对路径或原解析诊断。

Cargo部分分别处理workspace/Cargo.lock和独立native/audio-alsa/Cargo.lock，保留文件SHA256、包名/版本/registry校验和；以metadata实际workspace依赖图区分direct/transitive，不把锁内全部包称直接依赖。license_declaration来自已下载包的声明，不验证法律兼容性；缺失/离线metadata失败明确unknown。Git/其他源不输出原始URL；不沿着其任意地址联网。

可选--upstream只读取显式公开目录中的go.mod/go.sum，不自动克隆或执行上游Go。区分require/indirect、本地replace与module/go.mod校验和；本地replace仍verified=false，不能用官方sum证明修改过的SDK。上游Go许可逐包/通告未知不填“通过”。同理，alsa/opus Rust封装声明不等同系统ALSA/Opus版本/许可；工具将后者单列未知，目标rootfs/包来源仍须主线提供。

实际main80db24e上的一次运行：workspace330个锁包、native163个（两个workspace可能重复，不加总为唯一依赖数），包声明许可均取得；固定上游128个require、2个本地replace。两处上游SDK/NCP LICENSE另已人工只读核对为Apache-2.0，根为AGPLv3，见docs/nkn/fixed-upstream.json；工具不借此给其余126模块许可背书。实际报告在setup-logs/member-a-dependency-inventory.json，未提交生成物。

本工具尚不做漏洞通告查询/离线通告库核验、目标包许可证鉴证、系统库版本读取或Go全传递许可审查；未知保持缺口。它不替代A-18秘密扫描器、发行SBOM签认或SEC-05最终验收。
