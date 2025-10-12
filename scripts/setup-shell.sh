#!/usr/bin/env bash

set -euo pipefail

# Create skel directory for new users
mkdir -p /etc/skel

# Install oh-my-zsh template for new users
git clone --depth=1 https://github.com/ohmyzsh/ohmyzsh.git /etc/skel/.oh-my-zsh

# Configure template .zshrc for new users
cat > /etc/skel/.zshrc << 'EOF'
# Path to oh-my-zsh installation
export ZSH="$HOME/.oh-my-zsh"

# Theme
ZSH_THEME="robbyrussell"

# Plugins
plugins=(git)

source $ZSH/oh-my-zsh.sh

# Zoxide initialization
if command -v zoxide &> /dev/null; then
  eval "$(zoxide init zsh)"
  alias z='zoxide'
fi
EOF

# Set zsh as default shell in /etc/passwd template
sed -i 's|/bin/bash|/usr/bin/zsh|g' /etc/default/useradd 2>/dev/null || true

echo "Shell setup complete!"
