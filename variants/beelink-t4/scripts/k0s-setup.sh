#!/bin/bash
set -euo pipefail

k0s kubectl wait --for=condition=Ready node --all --timeout=300s

mkdir -p /home/dev/.kube
k0s kubeconfig admin > /home/dev/.kube/config
chmod 600 /home/dev/.kube/config
chown -R dev:dev /home/dev/.kube

k0s kubectl apply -f /etc/k0s/urunc-runtimeclass.yaml

touch /var/lib/k0s/.setup-complete
