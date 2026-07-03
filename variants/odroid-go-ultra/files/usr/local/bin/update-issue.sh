#!/bin/bash
# Set console font - Terminus 12px, compact and readable on 854x480
setfont ter-v12b 2>/dev/null

# Fedora ships /etc/issue as a symlink to /usr/lib/issue, which is
# read-only under composefs. Write to a temp file and mv it into place -
# that replaces the symlink with a real file in the writable /etc overlay.
{
    echo ""
    echo "  ODROID GO ULTRA"
    echo "  $(uname -srm)"
    echo ""
    ip -4 addr show | awk '/inet / {printf "  %-8s %s\n", $NF, $2}'
    echo ""
    df -h /sysroot | tail -n1 | awk '{print "  disk: " $3 " / " $2 " (" $5 ")"}'
    echo ""
} > /etc/issue.new && mv -f /etc/issue.new /etc/issue
