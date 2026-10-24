# Sourced by Makefile; paths are arguments so tests need no access to /boot.
_armbian_install() (
    set -eu
    source_dir=$1
    system_root=${2%/}
    overlay_tool=$3
    boot_dir=$system_root/boot
    env_file=$boot_dir/armbianEnv.txt
    release_file=$system_root/etc/armbian-release
    name=cubie-a7z-pwm-fan
    destination=$boot_dir/overlay-user/$name.dtbo

    fail() { printf 'error: %s\n' "$*" >&2; exit 1; }
    setting() { sed -n "s/^$2=//p" "$1" | tr -d "\"'"; }
    has_overlay() {
        setting "$env_file" user_overlays | tr ' \t' '\n' | grep -Fxq "$name"
    }
    as_root() {
        if [ "$(id -u)" = 0 ]; then "$@"; else sudo -- "$@"; fi
    }
    other_overlays() {
        setting "$env_file" user_overlays | tr ' \t' '\n' | sed "/^$name\$/d; /^\$/d"
    }

    [ "$(uname -s)" = Linux ] || fail 'Armbian on Linux is required.'
    [ -r "$release_file" ] || fail 'Armbian release information is missing.'
    [ "$(setting "$release_file" BOARD)" = cubie-a7z ] || fail 'Only Cubie A7Z is supported.'
    [ "$(setting "$release_file" LINUXFAMILY)" = sun60iw2 ] || fail 'Only the sun60iw2 vendor BSP is supported.'
    model_file=$system_root/sys/firmware/devicetree/base/model
    [ -r "$model_file" ] && [ "$(tr -d '\000' < "$model_file")" = 'Radxa Cubie A7Z' ] || fail 'Running board is not Cubie A7Z.'
    [ ! -L "$env_file" ] && [ -f "$env_file" ] && [ -r "$env_file" ] || fail 'armbianEnv.txt must be a readable regular file.'
    [ ! -L "$boot_dir/overlay-user" ] || fail 'overlay-user must not be a symbolic link.'
    [ ! -e "$boot_dir/overlay-user" ] || [ -d "$boot_dir/overlay-user" ] || fail 'overlay-user must be a directory.'
    [ ! -L "$destination" ] || fail 'The target overlay must not be a symbolic link.'
    [ ! -e "$destination" ] || [ -f "$destination" ] || fail 'The target overlay must be a regular file.'
    [ "$(grep -c '^user_overlays=' "$env_file" || true)" -le 1 ] || fail 'Duplicate user_overlays entries.'

    fan_available=no
    for hwmon in "$system_root"/sys/class/hwmon/hwmon*; do
        if [ -r "$hwmon/name" ] && [ "$(cat "$hwmon/name")" = pwmfan ] && [ -r "$hwmon/pwm1" ]; then
            fan_available=yes
        fi
    done
    if [ "$fan_available" = yes ] && ! has_overlay; then
        printf '%s\n' 'pwmfan is already available; no overlay is needed.'
        exit 0
    fi
    kernel=$(uname -r)
    case "$kernel" in
        6.6.*-vendor-sun60iw2) ;;
        *) fail 'Only the sun60iw2 vendor 6.6 kernel is supported.' ;;
    esac
    dtb=$(setting "$env_file" fdtfile)
    [ "$dtb" = allwinner/sun60i-a733-cubie-a7z.dtb ] || fail 'Unexpected board DTB.'
    [ -r "$boot_dir/dtb/$dtb" ] || fail 'Board DTB is missing.'
    [ -r "$boot_dir/boot.cmd" ] && grep -q '^setenv overlay_error' "$boot_dir/boot.cmd" || fail 'The boot script does not support user overlays.'
    [ -x "$overlay_tool" ] || fail 'armbian-add-overlay is missing.'
    for tool in dtc fdtoverlay fdtget; do
        command -v "$tool" >/dev/null || fail 'Install device-tree-compiler first.'
    done
    compiler=dtc
    kernel_dtc=$system_root/lib/modules/$kernel/build/scripts/dtc
    if [ -d "$kernel_dtc" ]; then
        [ -x "$kernel_dtc/dtc" ] || fail 'The kernel headers contain an unusable dtc.'
        compiler=$kernel_dtc/dtc
    fi
    [ "$(fdtget -t s "$boot_dir/dtb/$dtb" / model)" = 'Radxa Cubie A7Z' ] || fail 'DTB model is not Cubie A7Z.'

    work=$(mktemp -d /tmp/cubie-a7z-overlay.XXXXXX)
    restore=no
    had_overlay=no
    cleanup() {
        result=$?
        trap - 0
        if [ "$restore" = yes ]; then
            printf 'Restoring boot configuration from %s\n' "$backup" >&2
            as_root cp -a "$backup/armbianEnv.txt" "$env_file" || printf 'Restore failed; use backup: %s\n' "$backup" >&2
            if [ "$had_overlay" = yes ]; then
                as_root cp -a "$backup/$name.dtbo" "$destination" || printf 'Overlay restore failed: %s\n' "$backup" >&2
            else
                as_root rm -f "$destination" || printf 'Remove incomplete overlay: %s\n' "$destination" >&2
            fi
        fi
        rm -rf "$work" || :
        exit "$result"
    }
    trap cleanup 0
    trap 'exit 130' INT
    trap 'exit 143' TERM
    cp "$source_dir/$name.dts" "$work/$name.dts" || fail 'Cannot read overlay source.'
    "$compiler" -@ -I dts -O dtb -o "$work/$name.dtbo" "$work/$name.dts" || fail 'Overlay compilation failed.'
    fdtoverlay -i "$boot_dir/dtb/$dtb" -o "$work/checked.dtb" "$work/$name.dtbo" || fail 'Overlay does not apply to the board DTB.'
    if has_overlay && [ -r "$destination" ] && cmp -s "$work/$name.dtbo" "$destination"; then
        if [ "$fan_available" = yes ]; then
            printf '%s\n' 'Overlay is already active; no changes made.'
        else
            printf '%s\n' 'Overlay is already installed; reboot manually to apply it.'
        fi
        exit 0
    fi

    sed '/^user_overlays=/d' "$env_file" > "$work/other.before"
    other_overlays > "$work/overlays.before"
    backup=$(as_root mktemp -d "$boot_dir/cubie-a7z-fanctl-backup.XXXXXX")
    as_root cp -a "$env_file" "$backup/armbianEnv.txt" || fail 'Cannot back up boot configuration.'
    if [ -e "$destination" ]; then
        as_root cp -a "$destination" "$backup/$name.dtbo" || fail 'Cannot back up the existing overlay.'
        had_overlay=yes
    fi
    printf 'Backup: %s\n' "$backup"
    restore=yes
    as_root "$overlay_tool" "$work/$name.dts" || fail 'Overlay installation failed.'
    [ "$(grep -c '^user_overlays=' "$env_file" || true)" = 1 ] && has_overlay || fail 'Overlay registration failed.'
    cmp -s "$work/$name.dtbo" "$destination" || fail 'Installed overlay does not match the validated output.'
    sed '/^user_overlays=/d' "$env_file" > "$work/other.after"
    other_overlays > "$work/overlays.after"
    cmp -s "$work/other.before" "$work/other.after" || fail 'Unrelated boot configuration was changed.'
    cmp -s "$work/overlays.before" "$work/overlays.after" || fail 'Other user overlays were changed.'
    restore=no
    printf '%s\n' 'Overlay installed. Reboot manually when convenient, then install/enable the fan service.'
)
