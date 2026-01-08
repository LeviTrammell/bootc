#!/bin/bash
# Setup WiFi Access Point for headless access
# This script configures a WiFi interface as an AP for direct phone/tablet connection

set -euo pipefail

WIFI_INTERFACE=""
AP_IP="192.168.100.1"
AP_NETMASK="24"

find_wifi_interface() {
    # Find first available WiFi interface
    for iface in /sys/class/net/*/wireless; do
        if [[ -d "$iface" ]]; then
            WIFI_INTERFACE=$(basename "$(dirname "$iface")")
            echo "Found WiFi interface: $WIFI_INTERFACE"
            return 0
        fi
    done
    echo "No WiFi interface found"
    return 1
}

start_ap() {
    if ! find_wifi_interface; then
        echo "Cannot start AP: no WiFi interface available"
        exit 1
    fi

    # Update hostapd config with correct interface
    sed -i "s/^interface=.*/interface=$WIFI_INTERFACE/" /etc/hostapd/hostapd.conf
    sed -i "s/^interface=.*/interface=$WIFI_INTERFACE/" /etc/dnsmasq.d/wifi-ap.conf

    # Stop NetworkManager management of this interface
    nmcli device set "$WIFI_INTERFACE" managed no 2>/dev/null || true

    # Bring up interface
    ip link set "$WIFI_INTERFACE" up

    # Assign static IP
    ip addr flush dev "$WIFI_INTERFACE"
    ip addr add "$AP_IP/$AP_NETMASK" dev "$WIFI_INTERFACE"

    # Enable IP forwarding for internet sharing
    echo 1 > /proc/sys/net/ipv4/ip_forward

    # Setup NAT for internet access through WAN interface
    # Find WAN interface (first non-loopback, non-wifi interface with default route)
    WAN_IFACE=$(ip route | grep default | awk '{print $5}' | head -1)
    if [[ -n "$WAN_IFACE" && "$WAN_IFACE" != "$WIFI_INTERFACE" ]]; then
        iptables -t nat -A POSTROUTING -o "$WAN_IFACE" -j MASQUERADE
        iptables -A FORWARD -i "$WIFI_INTERFACE" -o "$WAN_IFACE" -j ACCEPT
        iptables -A FORWARD -i "$WAN_IFACE" -o "$WIFI_INTERFACE" -m state --state RELATED,ESTABLISHED -j ACCEPT
        echo "NAT configured via $WAN_IFACE"
    fi

    # Start hostapd
    hostapd -B /etc/hostapd/hostapd.conf

    echo "WiFi AP started on $WIFI_INTERFACE"
    echo "SSID: $(grep '^ssid=' /etc/hostapd/hostapd.conf | cut -d= -f2)"
    echo "IP: $AP_IP"
}

stop_ap() {
    # Kill hostapd
    pkill hostapd || true

    if find_wifi_interface; then
        # Flush IP
        ip addr flush dev "$WIFI_INTERFACE" 2>/dev/null || true

        # Return to NetworkManager management
        nmcli device set "$WIFI_INTERFACE" managed yes 2>/dev/null || true
    fi

    # Clean up iptables rules
    iptables -t nat -F POSTROUTING 2>/dev/null || true
    iptables -F FORWARD 2>/dev/null || true

    echo "WiFi AP stopped"
}

case "${1:-start}" in
    start)
        start_ap
        ;;
    stop)
        stop_ap
        ;;
    restart)
        stop_ap
        sleep 2
        start_ap
        ;;
    *)
        echo "Usage: $0 {start|stop|restart}"
        exit 1
        ;;
esac
