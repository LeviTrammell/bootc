#! /usr/bin/env bash

set -euo pipefail

dnf install -y --best \
  arm-image-installer \
  curl \
  distrobox \
  git \
  gnupg2 \
  gpg \
  wget \
  wl-clipboard \
  zsh \
  zoxide \
  go-task \
  podman \
  podman-remote \
  openssh-server \
  sshfs \
  qemu-img \
  policycoreutils-python-utils

dnf clean all
