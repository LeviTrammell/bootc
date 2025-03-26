#!/bin/bash

check() {
    return 0
}

depends() {
    echo "base"
    return 0
}

install() {
    inst_multiple growpart resize2fs sfdisk touch
    inst_hook pre-mount 10 "$moddir/growfs.sh"
}

