#!/bin/bash

HOSTPREFIX="02"     # hex, two digits only
DEVICEPREFIX="06"   # hex, two digits only

if [ -f /sys/firmware/devicetree/base/serial-number ]; then
  SERIAL=$(tr -d '\0' < /sys/firmware/devicetree/base/serial-number)
fi

if [ -z "$SERIAL" ]; then
  SERIAL=$(cat /etc/machine-id | tr -d '\n')
fi

if [ -z "$SERIAL" ]; then
  SERIAL="0000000000000000"
fi

## calculate MAC addresses
padded='00000000000000'$SERIAL
for i in -10 -8 -6 -4 -2; do
    basemac=$basemac':'${padded: $i:2}
done
hostmac=$HOSTPREFIX$basemac
devmac=$DEVICEPREFIX$basemac

gadget=/sys/kernel/config/usb_gadget/pi4

if [[ ! -e "/etc/usb-gadgets/net-ecm" ]]; then
    echo "No such config, net-ecm, found in /etc/usb-gadgets"
    exit 1
fi

source /etc/usb-gadgets/net-ecm

mkdir -p ${gadget}
echo "${vendor_id}" > ${gadget}/idVendor
echo "${product_id}" > ${gadget}/idProduct
echo "${bcd_device}" > ${gadget}/bcdDevice
echo "${usb_version}" > ${gadget}/bcdUSB

if [ ! -z "${device_class}" ] ; then
    echo "${device_class}" > ${gadget}/bDeviceClass
    echo "${device_subclass}" > ${gadget}/bDeviceSubClass
    echo "${device_protocol}" > ${gadget}/bDeviceProtocol
fi

mkdir -p ${gadget}/strings/0x409
echo "${manufacturer}" > ${gadget}/strings/0x409/manufacturer
echo "${product}" > ${gadget}/strings/0x409/product
echo "${SERIAL}" > ${gadget}/strings/0x409/serialnumber


mkdir ${gadget}/configs/c.1
echo "${power}" > ${gadget}/configs/c.1/MaxPower

mkdir -p ${gadget}/configs/c.1/strings/0x409
echo "${config1}" > ${gadget}/configs/c.1/strings/0x409/configuration

mkdir -p ${gadget}/functions/ecm.usb0
echo "${devmac}" > ${gadget}/functions/ecm.usb0/dev_addr
echo "${hostmac}" > ${gadget}/functions/ecm.usb0/host_addr

ln -s ${gadget}/functions/ecm.usb0 ${gadget}/configs/c.1/

ls /sys/class/udc > ${gadget}/UDC

udevadm settle -t 5 || :

ip addr add 10.55.0.1/29 dev usb0
ip link set usb0 up
systemctl restart dnsmasq-usb0
