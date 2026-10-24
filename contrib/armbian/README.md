# Armbian：Cubie A7Z 风扇接口

适用于 Cubie A7Z 的 Armbian `sun60iw2` vendor 6.6 内核。
其他板型或主线内核不适用。

## 安装

在源码根目录安装工具并执行：

```sh
sudo apt-get update
sudo apt-get install make device-tree-compiler
make armbian-install
```

命令会检查系统和设备树，在修改前备份启动配置，再安装风扇 overlay。
已提供风扇接口或已安装相同 overlay 时会跳过；不会替换主 DTB，也不会自动重启。
备份位于命令输出的 `/boot/cubie-a7z-fanctl-backup.*` 目录。

安装后手动重启：

```sh
sudo reboot
```

重连后检查风扇接口：

```sh
for h in /sys/class/hwmon/hwmon*; do
    [ ! -r "$h/name" ] || { printf '%s: ' "$h"; cat "$h/name"; }
done
```

应出现 `pwmfan`。确认风扇转动后，按[主 README](../../README.md)安装并启用服务，
再观察降速后风扇仍能正常转动。不要让其他服务或内核温控同时控制该风扇。

## 回退

如已启用服务，先停止并禁用；然后编辑 `user_overlays`，仅移除 `cubie-a7z-pwm-fan`，保留其他条目：

```sh
sudo systemctl disable --now cubie-a7z-fanctl
sudoedit /boot/armbianEnv.txt
sudo rm -f /boot/overlay-user/cubie-a7z-pwm-fan.dtbo
sudo reboot
```

如需恢复备份，应先检查备份后新增的启动配置，避免覆盖需要保留的改动。
