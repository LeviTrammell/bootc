#!/bin/bash

dnf install -y dnsmasq
dnf clean all

touch /etc/fstab
grep -q configfs /etc/fstab || echo 'configfs /sys/kernel/config configfs defaults 0 0' >> /etc/fstab

# nmcli connection add type ethernet ifname usb0 con-name usb-gadget \
#   ipv4.method manual ipv4.addresses 10.55.0.1/29 \
#   ipv4.dns 10.55.0.1 ipv4.never-default yes
# nmcli connection modify usb-gadget connection.autoconnect yes

ip addr add 10.55.0.1/29 dev usb0
ip link set usb0 up

systemctl disable dnsmasq.service
systemctl enable dnsmasq-usb0.service
systemctl enable usb-gadget.service
