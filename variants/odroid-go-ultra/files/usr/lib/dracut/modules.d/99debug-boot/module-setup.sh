#!/bin/bash
check() { return 0; }
depends() { return 0; }
install() {
    inst_hook pre-mount 99 "$moddir/debug-log.sh"
    inst_hook pre-pivot 99 "$moddir/debug-log.sh"
    inst_hook initqueue/timeout 99 "$moddir/debug-log.sh"
}
