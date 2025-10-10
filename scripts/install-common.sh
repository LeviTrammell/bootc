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
  zellij \
  go-task \
  podman \
  podman-remote \
  openssh-server \
  sshfs \
  qemu-img \
  NetworkManager-wifi \
  wpa_supplicant \
  fuse-overlayfs \
  e2fsprogs

dnf clean all
