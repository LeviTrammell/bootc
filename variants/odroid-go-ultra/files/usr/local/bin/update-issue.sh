#!/bin/bash
# Set console font - Terminus 12px, compact and readable on 854x480
setfont ter-v12b 2>/dev/null

{
    echo ""
    echo "  ODROID GO ULTRA"
    echo "  $(uname -srm)"
    echo ""
    ip -4 addr show | awk '/inet / {printf "  %-8s %s\n", $NF, $2}'
    echo ""
    df -h / | tail -n1 | awk '{print "  disk: " $3 " / " $2 " (" $5 ")"}'
    echo ""
} > /etc/issue
