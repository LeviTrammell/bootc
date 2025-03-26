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
  podman \
  openssh-server \
  sshfs

dnf clean all
