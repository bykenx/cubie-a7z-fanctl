# cubie-a7z-fanctl

根据 CPU/SoC 温度，通过 Linux hwmon sysfs 接口自动控制 Cubie A7Z 的 PWM 风扇。
支持线性调速、设备重新发现、温度失效保护，以及 SIGTERM/SIGINT 退出处理。

面向 Debian 13（trixie）arm64，支持 Radxa 原版 Debian 与基于 Debian 13 的 Armbian，采用 MIT 许可证。

## 使用

### 环境要求

- Radxa 原版 Debian 13 或基于 Debian 13 的 Armbian，arm64，使用 systemd 管理服务。
- 风扇硬件、内核驱动和设备树已配置，hwmon 暴露可写的 `pwm1` 节点。
- 至少一个可读取的 CPU/SoC 温度节点。

其他具有兼容 hwmon 接口的 Linux 系统也可从源码构建使用。
本程序不加载驱动或修改设备树；使用前应停止控制同一风扇的其他服务。

### 系统安装差异

| 系统主页 | 已验证镜像 |
|---|---|
| [Radxa 原版 Debian](https://github.com/radxa-build/radxa-a733) | [Trixie CLI（rsdk-t6）](https://github.com/radxa-build/radxa-a733/releases/download/rsdk-t6/radxa-a733_trixie_cli_t6.output_512.img.xz) |
| [Armbian](https://armbian.cn/boards/cubie-a7z) | [Trixie vendor minimal](https://dl.armbian.com/cubie-a7z/Trixie_vendor_minimal) |

> [!IMPORTANT]
> **Radxa 原版 Debian**：已验证的镜像已提供风扇接口，直接按下文安装并启用服务。
>
> **Armbian**：已提供 `pwmfan` 和 `pwm1` 时可直接安装；接口缺失时，先按 [Armbian 适配说明](contrib/armbian/README.md)运行 `make armbian-install`，手动重启后再安装服务。

Armbian 镜像链接为滚动下载入口；附带的 overlay 适用于 `sun60iw2` vendor 6.6 内核。
两种系统共用相同的安装包、默认配置及服务管理命令，后续流程一致。

### 下载与安装

在本仓库的 GitHub **Releases** 页面选择所需发布，下载安装包和校验和：

| 文件 | 用途 |
|---|---|
| `cubie-a7z-fanctl_<version>_arm64.deb` | Debian 安装包，推荐使用 |
| `cubie-a7z-fanctl-<version>-linux-arm64.tar.gz` | 二进制压缩包，包含默认配置、服务文件、手册和许可证 |
| `SHA256SUMS` | 上述文件的 SHA-256 校验和 |

两个包均面向 Debian 13 arm64。二进制使用 glibc 动态链接，不适用于 Alpine/musl 或 macOS。
`<version>` 表示对应文件名中的版本部分，deb 包的版本包含 Debian 修订号。
运行命令前，请按实际下载的文件名替换占位符。将所选安装包及 `SHA256SUMS` 放在同一目录，先检查完整性：

```sh
sha256sum --ignore-missing --check SHA256SUMS
```

**方式一：安装 deb 包**

```sh
sudo apt-get install "./cubie-a7z-fanctl_<version>_arm64.deb"
```

随后按下方说明检查配置并启用服务。

**方式二：直接使用二进制压缩包**

```sh
tar -xzf "cubie-a7z-fanctl-<version>-linux-arm64.tar.gz"
cd "cubie-a7z-fanctl-<version>-linux-arm64"
./cubie-a7z-fanctl --check-config --config config/config.toml
sudo ./cubie-a7z-fanctl --config config/config.toml
```

最后一条命令在前台持续运行，Ctrl+C 会触发退出 PWM 写入。
需要 systemd 管理时，停止前台进程，然后手动安装：

```sh
sudo install -m 0755 cubie-a7z-fanctl /usr/sbin/cubie-a7z-fanctl
sudo install -d /etc/cubie-a7z-fanctl
if ! sudo test -e /etc/cubie-a7z-fanctl/config.toml; then
    sudo install -m 0644 config/config.toml /etc/cubie-a7z-fanctl/config.toml
fi
sudo install -D -m 0644 cubie-a7z-fanctl.service /usr/lib/systemd/system/cubie-a7z-fanctl.service
sudo install -D -m 0644 cubie-a7z-fanctl.8 /usr/share/man/man8/cubie-a7z-fanctl.8
sudo install -d /usr/share/doc/cubie-a7z-fanctl
sudo install -m 0644 README.md LICENSE THIRD-PARTY-LICENSES.txt /usr/share/doc/cubie-a7z-fanctl/
sudo systemctl daemon-reload
```

再按下方步骤启动服务。两种安装方式选择一种；手动安装的文件不由 dpkg 管理。

### 默认配置

[默认配置](config/config.toml)已在 Radxa 原版 Debian 和完成上述适配的 Armbian 上实机验证：
自动查找 hwmon 名称为 `pwmfan` 的风扇，依次读取 CPU 温度节点 zone0 和 zone3。
使用相同内核与驱动的 A7Z 通常无需修改，检查配置后即可启动服务。

需要调整调速曲线时，编辑 `/etc/cubie-a7z-fanctl/config.toml`：

| 配置项 | 默认值 | 说明 |
|---|---|---|
| `temp_paths` | thermal_zone0/temp、thermal_zone3/temp（完整路径见默认配置） | 按顺序读取，使用第一个有效温度；至少一个绝对路径 |
| `temp_min` / `temp_max` | 40.0 / 75.0 | 摄氏度调速边界，有限且前者小于后者 |
| `min_pwm` / `max_pwm` | 60 / 255 | PWM 两端，前者不大于后者 |
| `poll_interval_seconds` | 2 | 正常调速周期 |
| `rediscovery_interval_seconds` | 10 | 无设备时扫描周期及同类故障日志节流间隔 |
| `disconnected_retry_seconds` | 5 | 断连或写入失败后重新发现的等待时间 |
| `fail_safe_pwm` | 255 | 所有温度节点失效时使用 |
| `shutdown_pwm` | 255 | 收到 SIGTERM/SIGINT 后尝试写入；省略即不写 |
| `preferred_hwmon_name` | `pwmfan` | 优先匹配的非空 hwmon name |
| `allow_pwm_fallback` | false | 首选设备缺失时，是否允许选择其他带 pwm1 的设备 |

所有 PWM 为 0..255 的整数，时间为正整数秒。除 `shutdown_pwm` 外，各项均须提供；
未知字段和非法值会报错并非零退出。TOML 不支持 `null`，禁用退出写入请删除或注释 `shutdown_pwm`。

温度节点采用整数毫摄氏度，例如 `55000` 表示 55°C，接受范围为 −40～150°C。
无效读数会继续尝试下一路径。温度不高于下限时输出 `min_pwm`，不低于上限时输出 `max_pwm`，
中间线性插值并四舍五入，例如 55°C 对应 PWM 144。
设备按路径排序，首选 name 精确匹配优先于 fallback；多候选时记录警告和选择结果。

### 启动与日常管理

检查配置后启用服务：

```sh
sudo /usr/sbin/cubie-a7z-fanctl --check-config
sudo systemctl enable --now cubie-a7z-fanctl
```

查看状态和日志：

```sh
systemctl status cubie-a7z-fanctl
sudo journalctl -u cubie-a7z-fanctl -b --no-pager
```

配置在启动时加载，修改后重启；需要停止调速时执行 stop：

```sh
sudo systemctl restart cubie-a7z-fanctl
sudo systemctl stop cubie-a7z-fanctl
```

服务正常运行时不重复写入相同 PWM，重新发现设备后会重新写入。温度全部失效时尝试写入
`fail_safe_pwm`，恢复后自动调速；PWM 不可用时记录错误并重试，同类故障日志会节流。
SIGTERM/SIGINT 会唤醒等待并尝试写入 `shutdown_pwm`；写入失败会报错并非零退出。
SIGKILL 或掉电无法执行退出写入。服务使用 `Restart=on-failure`，显式 stop 后保持停止。

调试时可使用 `--check-config` 检查配置，或使用 `--once` 执行一个周期并保留 PWM 结果。
完整参数、互斥规则和退出状态见 `cubie-a7z-fanctl --help` 或 `man cubie-a7z-fanctl`。

### 升级与卸载

使用 deb 包时，升级安装新版本 deb。配置由 Debian conffile 机制管理，保留管理员修改，冲突时由 dpkg 提示处理。
运行中的服务升级后会重启，已停止的服务保持停止。

```sh
sudo apt-get install "./cubie-a7z-fanctl_<version>_arm64.deb"
sudo apt-get remove cubie-a7z-fanctl  # 停止服务，保留配置
sudo apt-get purge cubie-a7z-fanctl   # 清除配置和服务启用状态
```

使用二进制压缩包手动安装时，升级前停止服务，替换二进制及服务文件，保留原配置，
执行 `sudo systemctl daemon-reload` 后重新启动。手动卸载：

```sh
sudo systemctl disable --now cubie-a7z-fanctl
sudo rm -f /usr/sbin/cubie-a7z-fanctl /usr/lib/systemd/system/cubie-a7z-fanctl.service
sudo rm -f /usr/share/man/man8/cubie-a7z-fanctl.8
sudo rm -rf /usr/share/doc/cubie-a7z-fanctl
sudo systemctl daemon-reload
```

上述命令保留 `/etc/cubie-a7z-fanctl/` 配置目录，可按需自行清除。

### 故障排查

更换内核、设备树或风扇驱动后，如果默认配置无法找到设备，可查看节点：

```sh
for z in /sys/class/thermal/thermal_zone*; do
    printf '%s: ' "$z"; cat "$z/type" "$z/temp"
done
for h in /sys/class/hwmon/hwmon*; do
    printf '%s: ' "$h"; cat "$h/name"
    ls "$h"/pwm1 "$h"/temp*_input 2>/dev/null || true
done
```

根据输出调整配置中的 `temp_paths` 和 `preferred_hwmon_name`。
平台驱动名称与 hwmon name 可能不同，`hwmon0` 等编号也不是稳定标识；程序会自动扫描编号。
**写错 PWM 节点可能控制其他硬件，不建议盲目启用 fallback。** 默认关闭 fallback，
找不到 `pwmfan` 时会等待并重扫，不选择其他 PWM 设备。

- **Permission denied**：检查 root 权限、节点权限、只读挂载和自定义 systemd drop-in；不要给整个 `/sys` 开放写权限。
- **找不到 pwm1**：检查内核驱动、设备树、驱动绑定及 hwmon name。仅有温度节点不代表支持风扇调速。
- **pwm1_enable**：程序只读取并记录，不自动修改。不同驱动语义不同，不能统一假定 1 表示手动模式；
  请参考运行内核的[驱动文档](https://docs.kernel.org/hwmon/pwm-fan.html)。
- **风扇不转**：停止服务，确认设备身份后手动验证。将 `hwmonX` 替换为已确认的目录：

  ```sh
  sudo systemctl stop cubie-a7z-fanctl
  cat /sys/class/hwmon/hwmonX/name
  cat /sys/class/hwmon/hwmonX/pwm1_enable  # 可不存在
  printf '255\n' | sudo tee /sys/class/hwmon/hwmonX/pwm1
  cat /sys/class/hwmon/hwmonX/pwm1
  ```

  PWM 为 255 仍不转时，检查电源、接线、设备树及驱动状态。
  更换风扇后也应确认最低起转 PWM，默认值 60 不保证适合所有风扇。不要盲写 `pwm1_enable`。
  PWM 节点不可用或不可写时，软件无法保证实际风扇转速。

## 开发

### 环境要求

| 工作 | 所需环境 |
|---|---|
| 构建与测试 Rust 程序 | Rust >=1.85、Cargo、Make、C 链接器及系统开发文件 |
| 格式和静态检查 | rustfmt、Clippy |
| 生成第三方许可证声明 | Python >=3.11 |
| 构建 deb 包 | Debian 13、build-essential、debhelper compat 13、Rust/Cargo、Python、Lintian |

Debian 包在 Debian 13 arm64 上原生构建，当前未配置交叉编译。
macOS 可编译和运行模拟测试，生成的 Mach-O 程序不能安装到 Debian，也不能控制 Linux sysfs。

在 Debian 上准备完整开发环境：

```sh
sudo apt-get update
sudo apt-get install build-essential debhelper cargo rustc rustfmt rust-clippy python3 lintian
```

使用 rustup 管理工具链时，通过 `rustup component add rustfmt clippy` 安装检查组件。
以下命令均在源码根目录执行。

### 构建与检查

```sh
make fetch   # 联网下载 Cargo.lock 锁定的依赖
make build   # 离线 release 构建
make check   # 格式检查、严格 Clippy、测试和 release 构建
```

二进制位于 `target/release/cubie-a7z-fanctl`。可先检查源码中的配置：

```sh
target/release/cubie-a7z-fanctl --check-config --config config/config.toml
```

仓库提交 `Cargo.lock`，不采用 vendor；依赖保存在用户 Cargo 缓存中。
首次构建需要先下载依赖，后续构建和测试使用 `--locked --offline`。
使用 rustup 时，可通过 `make check CARGO="cargo +1.85.0"` 验证最低版本，
须先安装对应工具链及其 rustfmt、Clippy 组件。

测试使用临时目录和模拟 I/O，覆盖配置、温度回退、设备发现、写入恢复、fail-safe 和日志节流；
子进程测试验证 `--once` 与 SIGTERM/SIGINT，不修改真实 `/sys`。
Armbian overlay 安装器测试使用 Python 3 和 `device-tree-compiler`，通过 `make armbian-test` 运行。

### Debian 打包

准备依赖缓存后执行：

```sh
make deb
```

该命令执行 `dpkg-buildpackage -us -uc -b` 和 Lintian，包输出到源码目录的上一级，
例如 `../cubie-a7z-fanctl_<version>_arm64.deb`。也可直接运行 `dpkg-buildpackage -us -uc -b`，
再运行 `make lintian`。二进制包安装到 `/usr/sbin`，unit 安装到 `/usr/lib/systemd/system`。

源码包采用 `3.0 (quilt)`；需要源码包时执行：

```sh
make source-archive
dpkg-buildpackage -us -uc -S
```

源码包不携带第三方依赖源码，构建前须准备 Cargo 缓存。打包过程离线，清理 `target/` 不会删除用户缓存。
这是本地分发方案，不代表符合 Debian 官方归档对 Rust 依赖拆包的惯例。

### 生成 Release 文件

在 Debian 13 arm64 构建机准备好依赖缓存后执行：

```sh
make release
```

该命令构建并检查 deb 包，将 deb、二进制压缩包和 `SHA256SUMS` 写入 `dist/`。
压缩包和 deb 使用同一次 release 构建的二进制及许可证声明。

### 依赖与许可证维护

更新依赖后检查锁文件及最低 Rust 版本兼容性：

```sh
cargo update
make fetch
make licenses
make check
```

`make licenses` 按当前目标的非测试依赖生成 `target/THIRD-PARTY-LICENSES.txt`，
合并相同文本并保留版权信息。该文件不提交到仓库，构建 deb 时自动生成并随包安装。
脚本支持 `--target TRIPLE`，仅用于选择许可证声明的目标平台。

### 项目结构与常用命令

| 路径 | 内容 |
|---|---|
| `src/` | 配置、温度读取、hwmon 扫描、控制器、CLI 和服务循环 |
| `tests/` | 集成测试和子进程测试 |
| `config/` | A7Z 默认配置 |
| `debian/` | Debian 打包、systemd unit 和手册 |
| `contrib/armbian/` | Armbian 风扇 overlay、安装工具和说明 |
| `scripts/` | 第三方许可证生成工具 |

`make help` 查看全部命令；`make fmt`、`make clippy`、`make test` 可单独检查，
`make clean` 清理构建产物。文本使用 UTF-8、LF 和文件末尾换行，`Makefile` 与 `debian/rules` 使用 tab。
